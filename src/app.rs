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
    pub fan_frame: usize,
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
            fan_frame: 0,
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

    pub fn fan_interval(&self) -> Duration {
        let load = self.fast.as_ref().map_or(0.0, |f| f.total_cpu / 100.0);
        Duration::from_millis((500.0 - 400.0 * f64::from(load)) as u64)
    }

    pub fn tick_fan(&mut self) {
        self.fan_frame = (self.fan_frame + 1) % 4;
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
    fn fan_speed_scales_with_load() {
        let mut a = App::new(false);
        let idle = a.fan_interval();
        let mut f = app_with_procs().fast.unwrap();
        f.total_cpu = 100.0;
        a.ingest(Snapshot::Fast(f));
        assert!(a.fan_interval() < idle);
        assert_eq!(a.fan_interval().as_millis(), 100);
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
