//! Sudo-free process scanner built directly on libproc.
//!
//! `sysinfo`'s full process refresh costs ~35 ms of CPU per pass on an M5
//! (785 processes, 10-thread pool, multiple syscalls per pid) — ~1.9% average
//! CPU at the 2 s fast cadence, blowing the < 0.5% budget on its own. One
//! `proc_pidinfo(PROC_PIDTASKINFO)` per pid costs ~1 ms per pass for the same
//! data (measured: 39 ms → 1 ms). Process names are fetched via `proc_name`
//! only when a pid is first seen and cached across scans.
//!
//! `pti_total_user`/`pti_total_system` are in `mach_absolute_time` units;
//! `mach_timebase_info` converts them to nanoseconds (verified against
//! `getrusage` self-times).

use std::collections::HashMap;
use std::time::Instant;

use crate::sampler::ProcInfo;

const PROC_PIDTASKINFO: libc::c_int = 4;

/// `struct proc_taskinfo` from `<sys/proc_info.h>`.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct ProcTaskInfo {
    pti_virtual_size: u64,
    pti_resident_size: u64,
    pti_total_user: u64,
    pti_total_system: u64,
    pti_threads_user: u64,
    pti_threads_system: u64,
    pti_policy: i32,
    pti_faults: i32,
    pti_pageins: i32,
    pti_cow_faults: i32,
    pti_messages_sent: i32,
    pti_messages_received: i32,
    pti_syscalls_mach: i32,
    pti_syscalls_unix: i32,
    pti_csw: i32,
    pti_threadnum: i32,
    pti_numrunning: i32,
    pti_priority: i32,
}

/// `struct mach_timebase_info` from `<mach/mach_time.h>` (declared directly:
/// the `libc` version is deprecated in favor of an extra crate).
#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

extern "C" {
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> libc::c_int;
    fn proc_listallpids(buffer: *mut libc::c_void, buffersize: libc::c_int) -> libc::c_int;
    fn proc_pidinfo(
        pid: libc::c_int,
        flavor: libc::c_int,
        arg: u64,
        buffer: *mut libc::c_void,
        buffersize: libc::c_int,
    ) -> libc::c_int;
    fn proc_name(pid: libc::c_int, buffer: *mut libc::c_void, buffersize: u32) -> libc::c_int;
}

struct Seen {
    cum_ns: u64,
    name: String,
}

pub struct ProcScanner {
    prev: HashMap<i32, Seen>,
    last_scan: Option<Instant>,
    /// mach_absolute_time units → nanoseconds.
    ns_per_unit: f64,
    pids: Vec<i32>,
}

impl Default for ProcScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcScanner {
    pub fn new() -> Self {
        let mut tb = MachTimebaseInfo { numer: 0, denom: 0 };
        unsafe { mach_timebase_info(&mut tb) };
        let ns_per_unit = if tb.denom == 0 {
            1.0
        } else {
            f64::from(tb.numer) / f64::from(tb.denom)
        };
        Self { prev: HashMap::new(), last_scan: None, ns_per_unit, pids: vec![0; 2048] }
    }

    fn list_pids(&mut self) -> usize {
        loop {
            let cap_bytes = (self.pids.len() * std::mem::size_of::<i32>()) as libc::c_int;
            let n = unsafe { proc_listallpids(self.pids.as_mut_ptr().cast(), cap_bytes) };
            if n < 0 {
                return 0;
            }
            let n = n as usize;
            if n < self.pids.len() {
                return n;
            }
            // Buffer may have been exactly filled; grow and retry.
            self.pids.resize(self.pids.len() * 2, 0);
        }
    }

    /// One pass over all visible pids. CPU% is the delta in cumulative
    /// user+system time since the previous scan, as a percentage of one core
    /// (may exceed 100 for multi-threaded processes). The first scan reports
    /// 0.0 for every process, like `sysinfo` before its second refresh.
    pub fn scan(&mut self) -> Vec<ProcInfo> {
        let now = Instant::now();
        let dt_ns = self
            .last_scan
            .map(|t| now.duration_since(t).as_nanos() as f64)
            .unwrap_or(f64::INFINITY)
            .max(1.0);
        self.last_scan = Some(now);

        let n = self.list_pids();
        let mut out = Vec::with_capacity(n);
        let mut next = HashMap::with_capacity(n);
        let sz = std::mem::size_of::<ProcTaskInfo>() as libc::c_int;

        for i in 0..n {
            let pid = self.pids[i];
            if pid <= 0 {
                continue;
            }
            let mut ti = ProcTaskInfo::default();
            let r = unsafe {
                proc_pidinfo(pid, PROC_PIDTASKINFO, 0, (&mut ti as *mut ProcTaskInfo).cast(), sz)
            };
            if r != sz {
                continue; // gone, or not visible to this user
            }
            let raw = ti.pti_total_user.saturating_add(ti.pti_total_system);
            let cum_ns = (raw as f64 * self.ns_per_unit) as u64;

            let (cpu, name) = match self.prev.remove(&pid) {
                // A shrinking cumulative time means the pid was reused.
                Some(seen) if cum_ns >= seen.cum_ns => {
                    let cpu = (cum_ns - seen.cum_ns) as f64 / dt_ns * 100.0;
                    (cpu as f32, seen.name)
                }
                _ => (0.0, read_name(pid)),
            };
            out.push(ProcInfo { pid, name: name.clone(), cpu, mem: ti.pti_resident_size });
            next.insert(pid, Seen { cum_ns, name });
        }
        self.prev = next; // entries of exited pids are dropped here
        out
    }
}

fn read_name(pid: i32) -> String {
    let mut buf = [0u8; 128];
    let r = unsafe { proc_name(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if r <= 0 {
        return format!("pid {pid}");
    }
    String::from_utf8_lossy(&buf[..r as usize]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_sees_own_busy_loop() {
        let mut scanner = ProcScanner::new();
        let me = std::process::id() as i32;

        let first = scanner.scan();
        let self_first = first.iter().find(|p| p.pid == me).expect("own pid visible");
        assert_eq!(self_first.cpu, 0.0, "first scan reports 0 cpu");
        assert!(self_first.mem > 0, "resident size populated");
        assert!(!self_first.name.is_empty());

        // Burn ~150 ms of CPU, then rescan: our own cpu% must register.
        let t0 = Instant::now();
        while t0.elapsed().as_millis() < 150 {
            std::hint::black_box((0..1000).sum::<u64>());
        }
        let second = scanner.scan();
        let self_second = second.iter().find(|p| p.pid == me).expect("own pid visible");
        assert!(
            self_second.cpu > 20.0 && self_second.cpu < 1000.0,
            "busy loop should register as cpu load, got {}",
            self_second.cpu
        );
    }

    #[test]
    fn scan_is_plausible() {
        let mut scanner = ProcScanner::new();
        let procs = scanner.scan();
        assert!(procs.len() > 50, "expected many processes, got {}", procs.len());
        assert!(procs.iter().all(|p| p.pid > 0));
    }
}
