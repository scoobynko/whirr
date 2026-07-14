use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System};

use super::{FastSnap, ProcInfo, Snapshot};

const TICK: Duration = Duration::from_secs(2);

pub fn run(tx: Sender<Snapshot>) {
    let mut sys = System::new();
    let mut networks = Networks::new_with_refreshed_list();
    let mut prev_totals: Option<(u64, u64, Instant)> = None;

    loop {
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
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

        let per_core: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        let total_cpu = if per_core.is_empty() {
            0.0
        } else {
            per_core.iter().sum::<f32>() / per_core.len() as f32
        };

        let mut processes: Vec<ProcInfo> = sys
            .processes()
            .values()
            .map(|p| ProcInfo {
                pid: p.pid().as_u32() as i32,
                name: p.name().to_string_lossy().into_owned(),
                cpu: p.cpu_usage(),
                mem: p.memory(),
            })
            .collect();
        processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        processes.truncate(50);

        let snap = FastSnap {
            per_core,
            total_cpu,
            processes,
            net_rx_rate: rx_rate,
            net_tx_rate: tx_rate,
            net_rx_total: rx_total,
            net_tx_total: tx_total,
            load_avg: System::load_average().one,
            mem_used: sys.used_memory(),
            mem_total: sys.total_memory(),
        };
        if tx.send(Snapshot::Fast(snap)).is_err() {
            return; // UI gone, exit thread
        }
        std::thread::sleep(TICK);
    }
}
