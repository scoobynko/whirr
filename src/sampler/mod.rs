mod fast;
mod medium;
mod slow;

use std::sync::mpsc::Sender;

pub enum Snapshot {
    Fast(FastSnap),
    #[allow(dead_code)]
    Medium(MediumSnap),
    #[allow(dead_code)]
    Slow(SlowSnap),
}

// Some fields below are not yet read by main.rs — only `total_cpu` drives the
// placeholder UI this task. Later panel tasks (CPU/network/processes) will
// consume the rest; the `#[allow(dead_code)]` keeps clippy quiet until then
// without altering the field names/types defined by the task contract.
#[allow(dead_code)]
pub struct ProcInfo {
    pub pid: i32,
    pub name: String,
    pub cpu: f32,
    pub mem: u64,
}

#[allow(dead_code)]
pub struct FastSnap {
    pub per_core: Vec<f32>,       // 0..100 per core, order = OS core order
    pub total_cpu: f32,           // 0..100
    pub processes: Vec<ProcInfo>, // sorted by cpu desc, max 50
    pub net_rx_rate: f64,         // bytes/sec
    pub net_tx_rate: f64,
    pub net_rx_total: u64, // session bytes
    pub net_tx_total: u64,
    pub load_avg: f64,
    pub mem_used: u64,
    pub mem_total: u64,
}

#[allow(dead_code)]
pub struct PowerSnap {
    pub cpu_w: f64,
    pub gpu_w: f64,
    pub ane_w: f64,
}

#[allow(dead_code)]
pub struct BatterySnap {
    pub percent: u8,
    pub charging: bool,
    pub cycles: u32,
    pub health_pct: Option<u8>,
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq)]
pub enum PressureLevel {
    Normal,
    Warn,
    Critical,
}

#[allow(dead_code)]
pub struct MemDetail {
    pub app: u64,
    pub wired: u64,
    pub compressed: u64,
    pub free: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub pressure: PressureLevel,
}

#[allow(dead_code)]
pub struct MediumSnap {
    pub temp_c: Option<f32>,
    pub power: Option<PowerSnap>,
    pub battery: Option<BatterySnap>,
    pub memory: Option<MemDetail>,
    pub uptime_secs: u64,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct PortInfo {
    pub port: u16,
    pub process: String,
    pub pid: i32,
}

#[allow(dead_code)]
pub struct SlowSnap {
    pub ports: Vec<PortInfo>,
    pub stale: bool,
}

pub fn spawn_samplers(tx: Sender<Snapshot>) {
    let tx_fast = tx.clone();
    std::thread::spawn(move || fast::run(tx_fast));

    let tx_medium = tx.clone();
    std::thread::spawn(move || medium::run(tx_medium));

    let tx_slow = tx.clone();
    std::thread::spawn(move || slow::run(tx_slow));
}
