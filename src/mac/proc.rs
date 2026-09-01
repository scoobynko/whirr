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
        Self { prev: HashMap::new(), last_scan: None, ns_per_unit }
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

        let pids = list_all_pids();
        let mut out = Vec::with_capacity(pids.len());
        let mut next = HashMap::with_capacity(pids.len());
        let sz = std::mem::size_of::<ProcTaskInfo>() as libc::c_int;

        for pid in pids {
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

const PROC_PIDVNODEPATHINFO: libc::c_int = 9;
const MAXPATHLEN: usize = 1024;
/// sizeof(struct vnode_info): struct vinfo_stat (136) + vi_type + vi_pad +
/// vi_fsid[2] — verified against <sys/proc_info.h>, see Task 1 Step 1.
const VNODE_INFO_SIZE: usize = 152;

/// `struct vnode_info_path` from `<sys/proc_info.h>`. The leading
/// `vnode_info` is opaque padding here — only the path matters.
#[repr(C)]
struct VnodeInfoPath {
    _vi: [u8; VNODE_INFO_SIZE],
    vip_path: [u8; MAXPATHLEN],
}

/// `struct proc_vnodepathinfo` from `<sys/proc_info.h>`.
#[repr(C)]
struct ProcVnodePathInfo {
    pvi_cdir: VnodeInfoPath,
    pvi_rdir: VnodeInfoPath,
}

/// `pid`'s current working directory, absolute. `None` for pids we may not
/// inspect (other users), dead pids, `/`, or any FFI failure. The full path is
/// returned rather than a basename because callers need to test it for `.git`.
pub fn cwd(pid: i32) -> Option<std::path::PathBuf> {
    let mut info: ProcVnodePathInfo = unsafe { std::mem::zeroed() };
    let sz = std::mem::size_of::<ProcVnodePathInfo>() as libc::c_int;
    let r = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDVNODEPATHINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            sz,
        )
    };
    if r != sz {
        return None;
    }
    let path = &info.pvi_cdir.vip_path;
    let end = path.iter().position(|&b| b == 0).unwrap_or(path.len());
    let s = std::str::from_utf8(&path[..end]).ok()?;
    if s.is_empty() || s == "/" {
        return None;
    }
    Some(std::path::PathBuf::from(s))
}

fn read_name(pid: i32) -> String {
    let mut buf = [0u8; 128];
    let r = unsafe { proc_name(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if r <= 0 {
        return format!("pid {pid}");
    }
    String::from_utf8_lossy(&buf[..r as usize]).into_owned()
}

// PROC_PIDTBSDINFO flavor; `proc_pidinfo` returns the struct size (136) on
// success. Verified live 2026-07-30.
const PROC_PIDTBSDINFO: libc::c_int = 3;

unsafe extern "C" {
    fn proc_pidpath(pid: libc::c_int, buf: *mut libc::c_void, len: u32) -> libc::c_int;
    /// Not in the `libc` crate; resolves a device number to its /dev name.
    fn devname(dev: libc::dev_t, mode: libc::mode_t) -> *const libc::c_char;
}

/// Layout of `struct proc_bsdinfo` from <sys/proc_info.h>. Field order matters —
/// this is read straight out of kernel memory.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct ProcBsdInfo {
    pbi_flags: u32,
    pbi_status: u32,
    pbi_xstatus: u32,
    pbi_pid: u32,
    pbi_ppid: u32,
    pbi_uid: u32,
    pbi_gid: u32,
    pbi_ruid: u32,
    pbi_rgid: u32,
    pbi_svuid: u32,
    pbi_svgid: u32,
    rfu_1: u32,
    pbi_comm: [u8; 16],
    pbi_name: [u8; 32],
    pbi_nfiles: u32,
    pbi_pgid: u32,
    pbi_pjobc: u32,
    e_tdev: u32,
    e_tpgid: u32,
    pbi_nice: i32,
    pbi_start_tvsec: u64,
    pbi_start_tvusec: u64,
}

/// `pid`'s executable path. One cheap syscall — unlike `args`, this does not
/// allocate an argv-sized buffer, which is why it is the right filter for
/// walking every pid on the system.
pub fn exec_path(pid: i32) -> Option<std::path::PathBuf> {
    let mut buf = [0u8; 4096];
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    let s = std::str::from_utf8(&buf[..n as usize]).ok()?;
    Some(std::path::PathBuf::from(s))
}

/// `pid`'s controlling terminal, e.g. `ttys021`, from one `PROC_PIDTBSDINFO`
/// call. The outer `None` means the pid could not be inspected at all (another
/// user's process, a dead pid, an FFI failure); the inner `None` means it has
/// no controlling terminal.
pub fn tty(pid: i32) -> Option<Option<String>> {
    let mut i = ProcBsdInfo::default();
    let sz = std::mem::size_of::<ProcBsdInfo>() as libc::c_int;
    let r = unsafe {
        proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &mut i as *mut _ as *mut libc::c_void, sz)
    };
    if r != sz {
        return None;
    }
    let tty = if i.e_tdev == 0 || i.e_tdev == u32::MAX {
        None
    } else {
        let p = unsafe { devname(i.e_tdev as libc::dev_t, libc::S_IFCHR) };
        if p.is_null() {
            None
        } else {
            let s = unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned();
            // devname yields "??" when it cannot resolve the device.
            if s.is_empty() || s == "??" { None } else { Some(s) }
        }
    };
    Some(tty)
}

/// `pid`'s parent, from the same `PROC_PIDTBSDINFO` call `tty` uses.
///
/// Walking this chain is how whirr finds the application hosting a terminal
/// session: the shell's parent is `login`, whose parent is the terminal app
/// itself. `None` means the pid could not be inspected.
pub fn ppid(pid: i32) -> Option<i32> {
    let mut i = ProcBsdInfo::default();
    let sz = std::mem::size_of::<ProcBsdInfo>() as libc::c_int;
    let r = unsafe {
        proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &mut i as *mut _ as *mut libc::c_void, sz)
    };
    (r == sz).then_some(i.pbi_ppid as i32)
}

/// Every visible pid. Grows its buffer until the kernel's answer fits.
pub fn list_all_pids() -> Vec<i32> {
    let mut buf = vec![0i32; 2048];
    loop {
        let cap = (buf.len() * std::mem::size_of::<i32>()) as libc::c_int;
        let n = unsafe { proc_listallpids(buf.as_mut_ptr().cast(), cap) };
        if n < 0 {
            return Vec::new();
        }
        let n = n as usize;
        if n < buf.len() {
            buf.truncate(n);
            return buf;
        }
        buf.resize(buf.len() * 2, 0);
    }
}

/// The kernel's cap on how big an argument area can be. Read once: it is a
/// boot-time constant, and a fresh `sysctl` per call would be a second syscall
/// for a number that never moves.
fn argmax() -> usize {
    use std::sync::OnceLock;
    static MAX: OnceLock<usize> = OnceLock::new();
    *MAX.get_or_init(|| crate::mac::sysctl::sysctl_u32("kern.argmax").unwrap_or(262_144) as usize)
}

/// `pid`'s command line, joined with spaces.
///
/// Deliberately not part of any full-system walk: this allocates an
/// argmax-sized buffer (256 KB by default) and copies the process's whole
/// argument *and* environment area. It is affordable for one pid the user
/// asked about, and nothing like affordable for 800 of them — which is why
/// `exec_path` exists and is what the scanners use.
///
/// `None` means the kernel would not answer: another user's process, a pid
/// that died between the listing and the read, or a hardened binary.
pub fn args(pid: i32) -> Option<String> {
    const KERN_PROCARGS2: libc::c_int = 49;
    let mut buf = vec![0u8; argmax()];
    let mut mib = [libc::CTL_KERN, KERN_PROCARGS2, pid];
    let mut len = buf.len();
    let r = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if r != 0 || len < 4 {
        return None;
    }
    buf.truncate(len);
    Some(parse_procargs2(&buf))
}

/// The argument vector inside a `KERN_PROCARGS2` blob, joined with spaces.
///
/// Layout: a 4-byte `argc`, the executable path, then NUL padding, then
/// exactly `argc` NUL-terminated arguments, then the environment — which is
/// why the count matters and a naive split on NUL would hand back the whole
/// environment as well.
fn parse_procargs2(buf: &[u8]) -> String {
    let argc = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    let rest = &buf[4..];
    // The exec path comes first and is not one of the `argc` arguments; skip
    // it, then the run of padding NULs that aligns what follows.
    let after_path = rest.iter().position(|&b| b == 0).map_or(rest.len(), |i| i + 1);
    let start = after_path + rest[after_path..].iter().take_while(|&&b| b == 0).count();
    rest[start..]
        .split(|&b| b == 0)
        .take(argc)
        .map(String::from_utf8_lossy)
        .collect::<Vec<_>>()
        .join(" ")
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

    #[test]
    fn cwd_of_self_is_the_full_current_dir() {
        let want = std::env::current_dir().expect("cwd readable");
        let got = super::cwd(std::process::id() as i32).expect("own cwd readable");
        // Compare canonicalised: /tmp is a symlink to /private/tmp on macOS.
        assert_eq!(
            got.canonicalize().unwrap(),
            want.canonicalize().unwrap(),
            "expected the full path, not a basename"
        );
        assert!(got.is_absolute(), "must be absolute so .git can be tested");
    }

    #[test]
    fn cwd_of_dead_pid_is_none() {
        assert_eq!(super::cwd(-1), None);
    }

    #[test]
    fn exec_path_of_self_is_the_test_binary() {
        let p = super::exec_path(std::process::id() as i32).expect("own exec path readable");
        assert!(p.is_absolute(), "must be absolute: {p:?}");
        // Cargo's test binary lives under target/; enough to prove it is a real path.
        assert!(p.exists(), "path must exist on disk: {p:?}");
    }

    #[test]
    fn exec_path_of_dead_pid_is_none() {
        assert_eq!(super::exec_path(-1), None);
    }

    #[test]
    fn tty_of_self_is_readable_and_well_shaped() {
        // The outer Some proves the pid was inspected at all. The inner value
        // may legitimately be None under a test harness with no controlling
        // terminal, so only its shape is pinned.
        let t = super::tty(std::process::id() as i32).expect("own pid must be inspectable");
        if let Some(name) = &t {
            assert!(name.starts_with("tty") || name.starts_with("cons"), "odd tty name: {name}");
        }
    }

    #[test]
    fn ppid_of_self_is_the_test_runners_parent() {
        let me = std::process::id() as i32;
        let parent = super::ppid(me).expect("own pid must be inspectable");
        assert!(parent > 0, "every process has a parent, got {parent}");
        assert_ne!(parent, me, "a process is not its own parent");
    }

    #[test]
    fn ppid_of_an_uninspectable_pid_is_none() {
        assert_eq!(super::ppid(-1), None);
    }

    #[test]
    fn the_parent_chain_terminates_at_launchd() {
        // The walk `host::detect` does. Two things must hold: it terminates,
        // and it tolerates an ancestor it cannot read — not every process up
        // the chain belongs to this user, and a walk that unwraps there would
        // panic on someone else's machine rather than simply finding nothing.
        let mut pid = std::process::id() as i32;
        let mut seen = 0;
        while pid > 1 && seen < 64 {
            match super::ppid(pid) {
                Some(parent) if parent != pid => pid = parent,
                // Unreadable, or its own parent: either way the walk is over.
                _ => break,
            }
            seen += 1;
        }
        assert!(seen < 64, "the walk did not terminate");
    }

    #[test]
    fn tty_of_dead_pid_is_none() {
        assert!(super::tty(-1).is_none(), "an uninspectable pid must be the outer None");
    }

    #[test]
    fn list_all_pids_includes_self() {
        let pids = super::list_all_pids();
        assert!(pids.len() > 10, "a live macOS box has many pids, got {}", pids.len());
        assert!(pids.contains(&(std::process::id() as i32)), "own pid missing");
    }

    #[test]
    fn procargs2_takes_the_arguments_and_stops_before_the_environment() {
        // argc, exec path, NUL padding, argc arguments, then the environment
        // — which a naive split on NUL would hand back as arguments too.
        let mut buf = 2u32.to_ne_bytes().to_vec();
        buf.extend(b"/bin/zsh\0\0\0");
        buf.extend(b"/bin/zsh\0-c\0");
        buf.extend(b"PATH=/usr/bin\0HOME=/Users/me\0");
        assert_eq!(super::parse_procargs2(&buf), "/bin/zsh -c");
    }

    #[test]
    fn procargs2_of_a_process_with_no_arguments_is_empty() {
        let mut buf = 0u32.to_ne_bytes().to_vec();
        buf.extend(b"/bin/sleep\0\0");
        buf.extend(b"PATH=/usr/bin\0");
        assert_eq!(super::parse_procargs2(&buf), "");
    }

    #[test]
    fn our_own_command_line_is_readable() {
        // The one thing a fixture cannot prove: that the sysctl is wired up
        // and the layout matches this kernel. The test binary's own argv
        // always contains its path.
        let mine = super::args(std::process::id() as i32).expect("own argv must be readable");
        assert!(mine.contains("whirr") || mine.contains("deps"), "odd argv: {mine:?}");
    }

}
