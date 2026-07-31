mod fast;
mod medium;
pub mod ports;
pub mod sessions;
mod slow;

use std::sync::mpsc::Sender;

pub enum Snapshot {
    Fast(FastSnap),
    Medium(MediumSnap),
    Slow(SlowSnap),
}

#[derive(Clone)]
pub struct ProcInfo {
    pub pid: i32,
    pub name: String,
    pub cpu: f32,
    pub mem: u64,
}

pub struct FastSnap {
    pub total_cpu: f32,           // 0..100, the mean across cores
    pub processes: Vec<ProcInfo>, // union of top 50 by cpu and top 50 by mem
    pub net_rx_rate: f64,         // bytes/sec
    pub net_tx_rate: f64,
    pub net_rx_total: u64, // bytes since whirr started (baseline-subtracted), not since boot
    pub net_tx_total: u64,
    pub load_avg: f64,
    pub mem_total: u64,
}

pub struct PowerSnap {
    pub cpu_w: f64,
    pub gpu_w: f64,
    pub ane_w: f64,
}

pub struct BatterySnap {
    pub percent: u8,
    pub charging: bool,
    pub cycles: u32,
    pub health_pct: Option<u8>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PressureLevel {
    Normal,
    Warn,
    Critical,
}

pub struct MemDetail {
    pub app: u64,
    pub wired: u64,
    pub compressed: u64,
    pub free: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub pressure: PressureLevel,
}

pub struct MediumSnap {
    pub temp_c: Option<f32>,
    pub power: Option<PowerSnap>,
    pub battery: Option<BatterySnap>,
    pub memory: Option<MemDetail>,
    pub uptime_secs: u64,
}

#[derive(Clone)]
pub struct PortInfo {
    pub port: u16,
    pub process: String,
    pub pid: i32,
}

pub struct SlowSnap {
    pub rows: Vec<crate::sampler::ports::PortRow>,
    pub sessions: Vec<crate::sampler::sessions::ClaudeSession>,
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
