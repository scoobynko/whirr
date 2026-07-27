use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::history::History;
use crate::mac::sysctl::SystemStatic;
use crate::sampler::{
    BatterySnap, FastSnap, MemDetail, MediumSnap, PortInfo, PowerSnap, PressureLevel, ProcInfo,
    SlowSnap, Snapshot,
};

pub enum Focus {
    Processes,
    Ports,
}

pub enum SortBy {
    Cpu,
    Mem,
}

/// The process table shows at most this many rows; selection clamps with it.
pub const MAX_VISIBLE_PROCS: usize = 10;

pub struct App {
    pub statics: SystemStatic,
    pub fast: Option<FastSnap>,
    pub medium: Option<MediumSnap>,
    pub slow: Option<SlowSnap>,
    pub cpu_hist: History<f32>,
    pub temp_hist: History<f32>,
    pub power_hist: History<(f64, f64, f64)>,
    pub net_hist: History<(f64, f64)>,
    pub focus: Focus,
    pub sort_by: SortBy,
    pub selected: usize,
    pub pending_kill: Option<(i32, String)>,
    pub message: Option<String>,
    pub no_fan: bool,
    /// The burst's inner-ring rotation in degrees, wrapped to `0.0..360.0`.
    /// The outer ring is its negation, so one accumulator drives both. Thermal:
    /// the hotter the machine, the faster it turns.
    pub fan_angle_deg: f32,
    pub should_quit: bool,
    pub dirty: bool,
}

impl App {
    pub fn new(no_fan: bool) -> Self {
        Self {
            statics: SystemStatic::read(),
            fast: None,
            medium: None,
            slow: None,
            cpu_hist: History::new(60),
            temp_hist: History::new(60),
            power_hist: History::new(60),
            net_hist: History::new(60),
            focus: Focus::Processes,
            sort_by: SortBy::Cpu,
            selected: 0,
            pending_kill: None,
            message: None,
            no_fan,
            fan_angle_deg: 0.0,
            should_quit: false,
            dirty: true,
        }
    }

    /// A pre-populated `App` for render tests and manual UI inspection: one
    /// `FastSnap` (a few processes, per-core loads), one `MediumSnap` (all
    /// `Some`, temp at 88.0 to exercise the amber threshold), and one
    /// `SlowSnap` (a few ports, not stale).
    pub fn demo() -> Self {
        let mut app = Self::new(false);
        app.ingest(Snapshot::Fast(FastSnap {
            per_core: vec![12.0, 45.0, 78.0, 30.0],
            total_cpu: 41.0,
            processes: vec![
                ProcInfo { pid: 101, name: "kernel_task".into(), cpu: 12.5, mem: 512_000 },
                ProcInfo { pid: 202, name: "WindowServer".into(), cpu: 8.3, mem: 256_000 },
                ProcInfo { pid: 303, name: "whirr".into(), cpu: 2.1, mem: 32_000 },
            ],
            net_rx_rate: 1_200_000.0,
            net_tx_rate: 300_000.0,
            net_rx_total: 500_000_000,
            net_tx_total: 120_000_000,
            load_avg: 2.35,
            mem_total: 16_000_000_000,
        }));
        app.ingest(Snapshot::Medium(MediumSnap {
            temp_c: Some(88.0),
            power: Some(PowerSnap { cpu_w: 6.4, gpu_w: 1.2, ane_w: 0.3 }),
            battery: Some(BatterySnap { percent: 76, charging: true, cycles: 120, health_pct: Some(97) }),
            memory: Some(MemDetail {
                app: 4_000_000_000,
                wired: 2_000_000_000,
                compressed: 1_000_000_000,
                free: 9_000_000_000,
                swap_used: 0,
                swap_total: 1_000_000_000,
                pressure: PressureLevel::Normal,
            }),
            uptime_secs: 3_600 * 5,
        }));
        app.ingest(Snapshot::Slow(SlowSnap {
            ports: vec![
                PortInfo { port: 22, process: "sshd".into(), pid: 1, project: None },
                PortInfo { port: 8080, process: "whirr-dev".into(), pid: 303, project: Some("my-app".into()) },
                PortInfo { port: 5432, process: "postgres".into(), pid: 55, project: None },
            ],
            stale: false,
        }));
        app
    }

    fn sort_procs(&mut self) {
        if let Some(f) = self.fast.as_mut() {
            match self.sort_by {
                SortBy::Cpu => f.processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
                SortBy::Mem => f.processes.sort_by_key(|p| std::cmp::Reverse(p.mem)),
            }
        }
    }

    pub fn ingest(&mut self, snap: Snapshot) {
        match snap {
            Snapshot::Fast(f) => {
                self.cpu_hist.push(f.total_cpu);
                self.net_hist.push((f.net_rx_rate, f.net_tx_rate));
                self.fast = Some(f);
                self.sort_procs();
            }
            Snapshot::Medium(m) => {
                if let Some(t) = m.temp_c {
                    self.temp_hist.push(t);
                }
                if let Some(p) = &m.power {
                    self.power_hist.push((p.cpu_w, p.gpu_w, p.ane_w));
                }
                self.medium = Some(m);
            }
            Snapshot::Slow(s) => self.slow = Some(s),
        }
        self.clamp_selection();
        self.dirty = true;
    }

    fn focused_len(&self) -> usize {
        match self.focus {
            Focus::Processes => self.visible_processes().len(),
            Focus::Ports => self.slow.as_ref().map_or(0, |s| s.ports.len()),
        }
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.focused_len().saturating_sub(1));
    }

    pub fn visible_processes(&self) -> &[ProcInfo] {
        self.fast.as_ref().map_or(&[], |f| {
            &f.processes[..f.processes.len().min(MAX_VISIBLE_PROCS)]
        })
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        self.dirty = true;
        self.message = None;

        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        // Ctrl-C must quit unconditionally, even while a kill confirmation is
        // pending — otherwise it's swallowed as a "cancel" below and the app
        // never exits on Ctrl-C during that prompt.
        if ctrl_c {
            self.should_quit = true;
            return;
        }

        if let Some((pid, name)) = self.pending_kill.clone() {
            match key.code {
                KeyCode::Char('y') => {
                    let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
                    if rc != 0 {
                        self.message = Some(format!("could not signal {name} ({pid})"));
                    }
                    self.pending_kill = None;
                }
                _ => self.pending_kill = None,
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Processes => Focus::Ports,
                    Focus::Ports => Focus::Processes,
                };
                self.selected = 0;
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.focused_len().saturating_sub(1));
            }
            KeyCode::Char('c') => {
                self.sort_by = SortBy::Cpu;
                self.sort_procs();
            }
            KeyCode::Char('m') => {
                self.sort_by = SortBy::Mem;
                self.sort_procs();
            }
            KeyCode::Char('k') => {
                if matches!(self.focus, Focus::Processes) {
                    if let Some(p) = self.visible_processes().get(self.selected) {
                        self.pending_kill = Some((p.pid, p.name.clone()));
                    }
                }
            }
            _ => {}
        }
    }

    /// Simulated Mac fan curve: lazy below ~55°C, ramping steeply toward
    /// 95°C — temperature is what actually drives real fans. Falls back to CPU
    /// load when the machine has no usable temp sensor.
    pub fn heat(&self) -> f32 {
        match self.medium.as_ref().and_then(|m| m.temp_c) {
            Some(t) => ((t - 55.0) / 40.0).clamp(0.0, 1.0),
            None => self.fast.as_ref().map_or(0.0, |f| (f.total_cpu / 100.0).clamp(0.0, 1.0)),
        }
    }

    /// Redraw interval for the burst: 125ms idle down to 60ms hot. The frame
    /// rate has to rise with the spin, not just the spin itself — each ring is
    /// 10-fold symmetric, so anything past 18°/frame aliases into a backwards
    /// spin. At 60ms/125ms this stays at 10.8°/3.2° per frame.
    pub fn fan_interval(&self) -> Duration {
        Duration::from_millis((125.0 - 65.0 * f64::from(self.heat())) as u64)
    }

    /// Advance the burst rotation over `dt` of real time: 360°/14s idle up to
    /// 360°/2s hot, matching the perceived range of the old stepped fan.
    pub fn tick_fan(&mut self, dt: Duration) {
        const COLD_DPS: f32 = 360.0 / 14.0;
        const HOT_DPS: f32 = 360.0 / 2.0;
        let dps = COLD_DPS + (HOT_DPS - COLD_DPS) * self.heat();
        self.fan_angle_deg = (self.fan_angle_deg + dps * dt.as_secs_f32()).rem_euclid(360.0);
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent};

    fn key(c: char) -> KeyEvent {
        KeyEvent::from(KeyCode::Char(c))
    }

    fn demo_fast() -> FastSnap {
        FastSnap {
            per_core: vec![50.0],
            total_cpu: 50.0,
            processes: vec![
                ProcInfo { pid: 1, name: "hog".into(), cpu: 90.0, mem: 100 },
                ProcInfo { pid: 2, name: "ram".into(), cpu: 10.0, mem: 900 },
            ],
            net_rx_rate: 0.0, net_tx_rate: 0.0, net_rx_total: 0, net_tx_total: 0,
            load_avg: 1.0, mem_total: 1,
        }
    }

    fn demo_medium(temp_c: f32) -> MediumSnap {
        MediumSnap {
            temp_c: Some(temp_c),
            power: None,
            battery: None,
            memory: None,
            uptime_secs: 3600,
        }
    }

    fn app_with_procs() -> App {
        let mut a = App::new(false);
        a.ingest(Snapshot::Fast(demo_fast()));
        a
    }

    #[test]
    fn sort_toggle_reorders() {
        let mut a = app_with_procs();
        assert_eq!(a.visible_processes()[0].name, "hog");
        a.on_key(key('m'));
        assert_eq!(a.visible_processes()[0].name, "ram");
        a.on_key(key('c'));
        assert_eq!(a.visible_processes()[0].name, "hog");
    }

    #[test]
    fn selection_clamps() {
        let mut a = app_with_procs();
        a.on_key(KeyEvent::from(KeyCode::Down));
        a.on_key(KeyEvent::from(KeyCode::Down));
        a.on_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(a.selected, 1); // only 2 processes
    }

    #[test]
    fn ctrl_c_quits_even_during_pending_kill() {
        let mut a = app_with_procs();
        a.on_key(key('k'));
        assert!(a.pending_kill.is_some());
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        a.on_key(ctrl_c);
        assert!(a.should_quit, "Ctrl-C must quit, not just cancel the kill prompt");
    }

    #[test]
    fn kill_flow_requires_confirm() {
        let mut a = app_with_procs();
        a.on_key(key('k'));
        assert!(a.pending_kill.is_some());
        a.on_key(key('n'));
        assert!(a.pending_kill.is_none());
    }

    #[test]
    fn fan_speed_follows_simulated_thermal_curve() {
        let mut a = App::new(false);
        // No data at all: idle spin at 125ms.
        assert_eq!(a.fan_interval().as_millis(), 125);

        // Temperature drives the curve when a sensor exists.
        let snap = |t: f32| MediumSnap {
            temp_c: Some(t),
            power: None,
            battery: None,
            memory: None,
            uptime_secs: 0,
        };
        a.ingest(Snapshot::Medium(snap(55.0)));
        assert_eq!(a.fan_interval().as_millis(), 125, "cool chip keeps idle speed");
        a.ingest(Snapshot::Medium(snap(95.0)));
        assert_eq!(a.fan_interval().as_millis(), 60, "hot chip spins fastest");
        a.ingest(Snapshot::Medium(snap(75.0)));
        let mid = a.fan_interval().as_millis();
        assert!(mid > 60 && mid < 125, "mid temp ramps between: {mid}");
    }

    #[test]
    fn fan_speed_falls_back_to_load_without_temp_sensor() {
        let mut a = App::new(false);
        let mut f = demo_fast();
        f.total_cpu = 100.0;
        a.ingest(Snapshot::Fast(f));
        assert_eq!(a.fan_interval().as_millis(), 60, "full load = fastest without sensor");
    }

    #[test]
    fn heat_tracks_temperature_and_falls_back_to_load() {
        let mut a = App::new(false);
        assert_eq!(a.heat(), 0.0, "no samples yet");
        a.ingest(Snapshot::Medium(demo_medium(40.0)));
        assert_eq!(a.heat(), 0.0, "40C is below the 55C floor");
        a.ingest(Snapshot::Medium(demo_medium(95.0)));
        assert_eq!(a.heat(), 1.0, "95C is the ceiling");
        a.ingest(Snapshot::Medium(demo_medium(75.0)));
        assert!((a.heat() - 0.5).abs() < 0.01, "75C is halfway");
    }

    #[test]
    fn fan_interval_ramps_from_125ms_to_60ms() {
        let mut a = App::new(false);
        assert_eq!(a.fan_interval(), Duration::from_millis(125));
        a.ingest(Snapshot::Medium(demo_medium(95.0)));
        assert_eq!(a.fan_interval(), Duration::from_millis(60));
    }

    #[test]
    fn tick_fan_never_turns_a_ring_more_than_eighteen_degrees_per_frame() {
        // Each ring is 10-fold symmetric: above 18 deg/frame it aliases and
        // appears to spin backwards. Must hold across the whole thermal range.
        for temp in [40.0, 55.0, 65.0, 75.0, 85.0, 95.0, 110.0] {
            let mut a = App::new(false);
            a.ingest(Snapshot::Medium(demo_medium(temp)));
            let dt = a.fan_interval();
            a.fan_angle_deg = 0.0;
            a.tick_fan(dt);
            assert!(
                a.fan_angle_deg < 18.0,
                "{temp}C turns {}deg per frame — aliases",
                a.fan_angle_deg
            );
        }
    }

    #[test]
    fn tick_fan_spins_faster_when_hot_and_wraps_at_360() {
        let mut cold = App::new(false);
        cold.ingest(Snapshot::Medium(demo_medium(40.0)));
        let mut hot = App::new(false);
        hot.ingest(Snapshot::Medium(demo_medium(95.0)));
        let dt = Duration::from_millis(100);
        cold.tick_fan(dt);
        hot.tick_fan(dt);
        assert!(hot.fan_angle_deg > cold.fan_angle_deg * 3.0, "hot fan should be much faster");

        let mut a = App::new(false);
        a.fan_angle_deg = 359.0;
        a.ingest(Snapshot::Medium(demo_medium(95.0)));
        a.tick_fan(Duration::from_millis(100));
        assert!(a.fan_angle_deg < 360.0, "angle must wrap, got {}", a.fan_angle_deg);
    }

    #[test]
    fn cold_fan_takes_about_fourteen_seconds_per_revolution() {
        let mut a = App::new(false);
        a.ingest(Snapshot::Medium(demo_medium(40.0)));
        a.tick_fan(Duration::from_secs(1));
        // 360/14 = 25.7 deg/s
        assert!((a.fan_angle_deg - 25.7).abs() < 0.5, "got {} deg/s", a.fan_angle_deg);
    }

    #[test]
    fn process_view_caps_at_ten() {
        let mut a = App::new(false);
        let procs: Vec<ProcInfo> = (0..30)
            .map(|i| ProcInfo {
                pid: i,
                name: format!("p{i}"),
                cpu: 30.0 - i as f32,
                mem: 100,
            })
            .collect();
        let mut f = demo_fast();
        f.processes = procs;
        a.ingest(Snapshot::Fast(f));
        assert_eq!(a.visible_processes().len(), 10);
        for _ in 0..15 {
            a.on_key(KeyEvent::from(KeyCode::Down));
        }
        assert_eq!(a.selected, 9);
    }
}
