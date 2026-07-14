use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use sysinfo::{Networks, System};

use crate::mac::proc::ProcScanner;

use super::{FastSnap, ProcInfo, Snapshot};

const TICK: Duration = Duration::from_secs(2);
const TOP_N: usize = 50;

/// Select the processes worth sending to the UI: the union of the top
/// `TOP_N` by CPU and the top `TOP_N` by memory, deduped by pid.
///
/// The old approach sorted by CPU and truncated to 50 before sending, so a
/// process with huge memory but near-zero CPU (e.g. an idle browser tab
/// process) would never appear in the snapshot at all — and `App::sort_procs`
/// re-sorting that already-truncated list by memory couldn't recover it.
/// Order doesn't matter here since the app re-sorts on ingest.
fn select_top(mut procs: Vec<ProcInfo>) -> Vec<ProcInfo> {
    if procs.len() <= TOP_N {
        return procs;
    }

    procs.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let mut seen: HashSet<i32> = procs.iter().take(TOP_N).map(|p| p.pid).collect();
    let mut result: Vec<ProcInfo> = procs.iter().take(TOP_N).cloned().collect();

    procs.sort_by_key(|p| std::cmp::Reverse(p.mem));
    for p in procs.into_iter().take(TOP_N) {
        if seen.insert(p.pid) {
            result.push(p);
        }
    }

    result
}

pub fn run(tx: Sender<Snapshot>) {
    let mut sys = System::new();
    let mut networks = Networks::new_with_refreshed_list();
    // Direct libproc scan instead of sysinfo's process refresh: ~1 ms of CPU
    // per pass instead of ~35 ms, which is the difference between blowing the
    // < 0.5% CPU budget and fitting comfortably inside it (see mac::proc).
    let mut scanner = ProcScanner::new();
    let mut prev_totals: Option<(u64, u64, Instant)> = None;
    // sysinfo's total_received/total_transmitted count from boot, not from
    // when whirr started. Capture the first sample as a baseline so the
    // reported "session" totals genuinely mean "since whirr started".
    let mut baseline_totals: Option<(u64, u64)> = None;

    loop {
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        networks.refresh(true);

        let (rx_total, tx_total) = networks.iter().fold((0u64, 0u64), |acc, (_, d)| {
            (acc.0 + d.total_received(), acc.1 + d.total_transmitted())
        });
        let now = Instant::now();
        let (rx_rate, tx_rate) = match prev_totals {
            Some((pr, pt, pi)) => {
                let dt = now.duration_since(pi).as_secs_f64().max(0.001);
                (
                    rx_total.saturating_sub(pr) as f64 / dt,
                    tx_total.saturating_sub(pt) as f64 / dt,
                )
            }
            None => (0.0, 0.0),
        };
        prev_totals = Some((rx_total, tx_total, now));

        let (base_rx, base_tx) = *baseline_totals.get_or_insert((rx_total, tx_total));
        let net_rx_total = rx_total.saturating_sub(base_rx);
        let net_tx_total = tx_total.saturating_sub(base_tx);

        let per_core: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        let total_cpu = if per_core.is_empty() {
            0.0
        } else {
            per_core.iter().sum::<f32>() / per_core.len() as f32
        };

        let processes = select_top(scanner.scan());

        let snap = FastSnap {
            per_core,
            total_cpu,
            processes,
            net_rx_rate: rx_rate,
            net_tx_rate: tx_rate,
            net_rx_total,
            net_tx_total,
            load_avg: System::load_average().one,
            mem_total: sys.total_memory(),
        };
        if tx.send(Snapshot::Fast(snap)).is_err() {
            return; // UI gone, exit thread
        }
        std::thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: i32, cpu: f32, mem: u64) -> ProcInfo {
        ProcInfo { pid, name: format!("p{pid}"), cpu, mem }
    }

    #[test]
    fn passthrough_when_at_or_under_top_n() {
        let procs: Vec<ProcInfo> = (0..TOP_N).map(|i| proc(i as i32, 1.0, 1)).collect();
        assert_eq!(select_top(procs.clone()).len(), procs.len());
    }

    #[test]
    fn includes_big_memory_idle_cpu_process_beyond_cpu_top_n() {
        // TOP_N processes all with high CPU and negligible memory...
        let mut procs: Vec<ProcInfo> =
            (0..TOP_N).map(|i| proc(i as i32, 50.0, 1)).collect();
        // ...plus one process that's idle on CPU but has huge memory. Under
        // the old cpu-sort-then-truncate approach this pid would never make
        // it into the snapshot.
        let whale_pid = 9_999;
        procs.push(proc(whale_pid, 0.0, u64::MAX));

        let selected = select_top(procs);
        assert!(selected.iter().any(|p| p.pid == whale_pid));
        // Union may exceed TOP_N by at most the number of mem-only entries.
        assert!(selected.len() <= TOP_N + 1);
    }

    #[test]
    fn dedups_processes_that_are_top_by_both_metrics() {
        let mut procs: Vec<ProcInfo> =
            (0..TOP_N).map(|i| proc(i as i32, i as f32, i as u64)).collect();
        // pid 0 is simultaneously the lowest by both cpu and mem in this set;
        // add a distinct top-mem outlier so both selection passes overlap
        // heavily and pid values repeat across passes.
        procs.push(proc(12345, 1000.0, 1000));
        let selected = select_top(procs);
        let mut pids: Vec<i32> = selected.iter().map(|p| p.pid).collect();
        let before = pids.len();
        pids.sort_unstable();
        pids.dedup();
        assert_eq!(pids.len(), before, "select_top must not emit duplicate pids");
    }
}
