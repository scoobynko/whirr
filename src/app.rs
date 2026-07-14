use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::history::History;
use crate::mac::sysctl::SystemStatic;
use crate::sampler::{FastSnap, MediumSnap, ProcInfo, SlowSnap, Snapshot};

pub enum Focus {
    Processes,
    Ports,
}

pub enum SortBy {
    Cpu,
    Mem,
}

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
            Focus::Processes => self.fast.as_ref().map_or(0, |f| f.processes.len()),
            Focus::Ports => self.slow.as_ref().map_or(0, |s| s.ports.len()),
        }
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.focused_len().saturating_sub(1));
    }

    pub fn visible_processes(&self) -> &[ProcInfo] {
        self.fast.as_ref().map_or(&[], |f| &f.processes)
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        self.dirty = true;
        self.message = None;

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

        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            _ if ctrl_c => self.should_quit = true,
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

    fn app_with_procs() -> App {
        let mut a = App::new(false);
        a.ingest(Snapshot::Fast(FastSnap {
            per_core: vec![50.0],
            total_cpu: 50.0,
            processes: vec![
                ProcInfo { pid: 1, name: "hog".into(), cpu: 90.0, mem: 100 },
                ProcInfo { pid: 2, name: "ram".into(), cpu: 10.0, mem: 900 },
            ],
            net_rx_rate: 0.0, net_tx_rate: 0.0, net_rx_total: 0, net_tx_total: 0,
            load_avg: 1.0, mem_used: 0, mem_total: 1,
        }));
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
}
