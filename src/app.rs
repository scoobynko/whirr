use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::history::History;
use crate::mac::sysctl::SystemStatic;
use crate::sampler::ports::{PortGroup, PortRow};
use crate::sampler::{
    BatterySnap, FastSnap, MemDetail, MediumSnap, PowerSnap, PressureLevel, ProcInfo, SlowSnap,
    Snapshot,
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
    ///
    /// The history buffers are pre-seeded with a short varied ramp before the
    /// "current" sample lands (below), so chart-rendering tests exercise a
    /// real filled chart body instead of a single point — a single-sample
    /// history can never show whether the newest data lands at the right
    /// edge of the chart (that gap is exactly how the spark::render
    /// left-align bug shipped invisibly).
    pub fn demo() -> Self {
        let mut app = Self::new(false);
        for v in [15.0_f32, 28.0, 52.0, 70.0, 58.0, 33.0, 22.0, 40.0, 63.0, 77.0, 49.0, 26.0] {
            app.cpu_hist.push(v);
        }
        for v in [
            (200_000.0_f64, 40_000.0),
            (450_000.0, 90_000.0),
            (900_000.0, 180_000.0),
            (600_000.0, 120_000.0),
            (1_500_000.0, 250_000.0),
            (300_000.0, 60_000.0),
        ] {
            app.net_hist.push(v);
        }
        app.ingest(Snapshot::Fast(FastSnap {
            per_core: vec![12.0, 45.0, 78.0, 30.0],
            total_cpu: 41.0,
            processes: vec![
                ProcInfo { pid: 101, name: "kernel_task".into(), cpu: 12.5, mem: 512_000 },
                ProcInfo { pid: 202, name: "WindowServer".into(), cpu: 8.3, mem: 256_000 },
                ProcInfo { pid: 303, name: "whirr".into(), cpu: 2.1, mem: 32_000 },
                ProcInfo { pid: 503, name: "claude".into(), cpu: 12.4, mem: 400_000 },
            ],
            net_rx_rate: 1_200_000.0,
            net_tx_rate: 300_000.0,
            net_rx_total: 500_000_000,
            net_tx_total: 120_000_000,
            load_avg: 2.35,
            mem_total: 16_000_000_000,
        }));
        for v in [42.0_f32, 55.0, 70.0, 82.0, 91.0, 76.0, 61.0, 48.0, 65.0, 84.0] {
            app.temp_hist.push(v);
        }
        for v in [
            (2.0_f64, 0.4, 0.05),
            (3.5, 0.7, 0.1),
            (5.0, 1.0, 0.2),
            (7.5, 1.6, 0.35),
            (6.0, 1.3, 0.25),
            (4.2, 0.9, 0.15),
        ] {
            app.power_hist.push(v);
        }
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
            rows: vec![
                PortRow {
                    group: PortGroup::Localhost,
                    label: "glassbook-frontend".into(),
                    pid: 501,
                    ports: vec![4206, 6006, 63643],
                },
                PortRow { group: PortGroup::Localhost, label: "axterio".into(), pid: 502, ports: vec![3000] },
                PortRow { group: PortGroup::Claude, label: "axterio".into(), pid: 503, ports: vec![65067] },
                PortRow {
                    group: PortGroup::Claude,
                    label: "ai-design-kit".into(),
                    pid: 504,
                    ports: vec![64033],
                },
                PortRow {
                    group: PortGroup::Other,
                    label: "ControlCenter".into(),
                    pid: 505,
                    ports: vec![5000, 7000],
                },
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
            Focus::Ports => self.slow.as_ref().map_or(0, |s| s.rows.len()),
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

    /// Live CPU for an arbitrary pid — the ports card joins claude rows against
    /// the fast tick. Unlike `visible_processes`, this searches the full list.
    pub fn cpu_of(&self, pid: i32) -> Option<f32> {
        self.fast.as_ref()?.processes.iter().find(|p| p.pid == pid).map(|p| p.cpu)
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
            KeyCode::Char('k') => match self.focus {
                Focus::Processes => {
                    if let Some(p) = self.visible_processes().get(self.selected) {
                        self.pending_kill = Some((p.pid, p.name.clone()));
                    }
                }
                // Only dev servers are killable. Ending a Claude session
                // mid-conversation or a system agent by a stray keypress is
                // not a thing this should make easy.
                Focus::Ports => {
                    if let Some(r) = self
                        .slow
                        .as_ref()
                        .and_then(|s| s.rows.get(self.selected))
                        .filter(|r| r.group == crate::sampler::ports::PortGroup::Localhost)
                    {
                        let n = r.ports.len();
                        let ports = if n == 1 { "port" } else { "ports" };
                        self.pending_kill = Some((r.pid, format!("{} ({n} {ports})", r.label)));
                    }
                }
            },
            _ => {}
        }
    }

    /// Simulated Mac fan curve: lazy below ~55°C, ramping steeply toward
    /// 95°C — temperature is what actually drives real fans. Falls back to CPU
    /// load when the machine has no usable temp sensor.
    pub fn heat(&self) -> f32 {
        match self.medium.as_ref().and_then(|m| m.temp_c) {
            // A NaN/infinite sensor read must not survive `clamp` (which
            // passes NaN through unchanged): `fan_interval` casts heat into a
            // millisecond duration, and a NaN there saturates to 0ms, spinning
            // the redraw loop at full speed. Fall back to the no-sensor path.
            Some(t) if t.is_finite() => ((t - 55.0) / 40.0).clamp(0.0, 1.0),
            _ => self.fast.as_ref().map_or(0.0, |f| {
                if f.total_cpu.is_finite() { (f.total_cpu / 100.0).clamp(0.0, 1.0) } else { 0.0 }
            }),
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
        // Each ring is 10-fold symmetric, so a step of 18 deg or more aliases
        // into a backwards spin. dt is measured, not nominal, and heat can rise
        // between the sleep and the tick, so the step is clamped rather than
        // assumed small. Losing a few degrees after a stall is invisible; a
        // backwards jump is not.
        const MAX_STEP: f32 = 17.0;
        let step = (dps * dt.as_secs_f32()).min(MAX_STEP);
        self.fan_angle_deg = (self.fan_angle_deg + step).rem_euclid(360.0);
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
    fn heat_falls_back_to_load_on_non_finite_temp() {
        // A malformed sensor read (NaN/inf) must not leak into fan_interval,
        // whose (125.0 - 65.0 * NaN) as u64 saturates to 0ms and would spin
        // the redraw loop at full speed pinning a core.
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut a = App::new(false);
            a.ingest(Snapshot::Medium(demo_medium(bad)));
            assert!(a.heat().is_finite(), "{bad} produced non-finite heat");
            assert_eq!(a.heat(), 0.0, "{bad}: no fast snapshot, falls back to 0.0");
            assert!(
                a.fan_interval() >= Duration::from_millis(60),
                "{bad}: fan_interval collapsed to a busy loop: {:?}",
                a.fan_interval()
            );
        }
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

        // Adversarial: dt is measured, not nominal, and heat can change
        // between the sleep and the tick (main.rs ingests a new sample before
        // calling tick_fan). At max heat, an unclamped step blows well past
        // 18 deg for a cold-cadence dt (the real-world trigger), and further
        // still for a stalled loop. The clamp must hold regardless of dt.
        // Note: each duration must have an unclamped step that is NOT a
        // multiple of 360°, else rem_euclid(360.0) wraps it to zero and the
        // assertion passes vacuously even without the clamp.
        for dt in [Duration::from_millis(125), Duration::from_secs(1), Duration::from_secs(7)] {
            let mut a = App::new(false);
            a.ingest(Snapshot::Medium(demo_medium(95.0)));
            a.fan_angle_deg = 100.0;
            a.tick_fan(dt);
            let delta = (a.fan_angle_deg - 100.0).rem_euclid(360.0);
            assert!(
                delta < 18.0,
                "max heat with dt={dt:?} turned {delta}deg in one frame — aliases"
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
        // One second of real ticks at the cold cadence (125ms), matching how
        // main.rs actually drives tick_fan — a single 1s dt would itself
        // exceed the per-frame clamp and no longer measure the rate.
        for _ in 0..8 {
            a.tick_fan(Duration::from_millis(125));
        }
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

    #[test]
    fn cpu_of_finds_pids_beyond_the_visible_window() {
        let mut a = App::new(false);
        let mut f = demo_fast();
        // More processes than MAX_VISIBLE_PROCS, with the target last.
        f.processes = (0..MAX_VISIBLE_PROCS as i32 + 5)
            .map(|i| ProcInfo { pid: 900 + i, name: format!("p{i}"), cpu: i as f32, mem: 0 })
            .collect();
        a.ingest(Snapshot::Fast(f));
        assert_eq!(a.cpu_of(900 + MAX_VISIBLE_PROCS as i32 + 4), Some(14.0));
        assert_eq!(a.cpu_of(1), None);
    }

    fn press(a: &mut App, c: char) {
        a.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }

    #[test]
    fn kill_works_on_a_localhost_port_row() {
        let mut a = App::demo();
        a.focus = Focus::Ports;
        a.selected = 0; // demo()'s first row is localhost/glassbook-frontend
        press(&mut a, 'k');
        let (pid, label) = a.pending_kill.clone().expect("localhost row must be killable");
        assert_eq!(pid, 501);
        assert!(label.contains("glassbook-frontend"), "dialog should name the process: {label}");
        assert!(label.contains('3'), "dialog should say how many ports die with it: {label}");
    }

    #[test]
    fn kill_is_inert_on_claude_and_other_rows() {
        for idx in [2usize, 4] {
            // demo() row 2 is a claude session, row 4 is ControlCenter.
            let mut a = App::demo();
            a.focus = Focus::Ports;
            a.selected = idx;
            press(&mut a, 'k');
            assert!(
                a.pending_kill.is_none(),
                "row {idx} must not be killable — a stray k must not end a session or a system agent"
            );
        }
    }
}
