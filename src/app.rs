use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::history::History;
use crate::mac::sysctl::SystemStatic;
use crate::sampler::ports::{self, PortGroup, PortRow};
use crate::settings::Settings;
use crate::sampler::claude_state::{Activity, SessionState};
use crate::sampler::sessions::ClaudeSession;
use crate::sampler::{
    BatterySnap, FastSnap, MemDetail, MediumSnap, PowerSnap, PressureLevel, ProcInfo, SlowSnap,
    Snapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Processes,
    Localhost,
    Sessions,
    Others,
}

impl Focus {
    /// Tab order. Doubles as the index space for `App::selected`, so a panel
    /// added here automatically gets its own cursor.
    pub const ALL: [Focus; 4] =
        [Focus::Processes, Focus::Localhost, Focus::Sessions, Focus::Others];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&f| f == self).expect("ALL covers every Focus")
    }

    fn next(self) -> Focus {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }
}

pub enum SortBy {
    Cpu,
    Mem,
}

/// A pending "which port did you mean?".
///
/// `o` used to open the lowest port of a row, on the theory that a server
/// owning several is reachable there. Storybook disproved it: its row carries
/// a Vite port, the Storybook UI, and an ephemeral socket, and the lowest of
/// those is not the page you want. Nothing in a port *number* says which one a
/// human meant, so when the row is ambiguous whirr asks instead of guessing.
#[derive(Clone, Debug)]
pub struct PortPick {
    /// The row's label, so the dialog can say whose ports these are.
    pub label: String,
    /// Candidates in the row's own order; the dialog numbers them from 1.
    pub ports: Vec<u16>,
}

/// Entries the picker can address with a digit key. Ten would need `0`, and a
/// row with this many browsable ports does not exist in practice.
pub const MAX_PICKABLE_PORTS: usize = 9;

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
    /// What the user has chosen. `theme` is derived from this and kept
    /// alongside it so widgets read one resolved value instead of resolving
    /// it per colour, per frame.
    pub settings: Settings,
    /// The palette every widget draws with. On `App` rather than a global so
    /// a render function that already has `&App` needs no extra argument.
    /// Always `settings.theme()` — see `apply_settings`.
    pub theme: crate::ui::theme::Theme,
    pub settings_open: bool,
    /// Set when a setting changes; the event loop drains it and writes the
    /// config. `App` does not touch the filesystem for the same reason it
    /// does not spawn `open`: tests press keys by the hundred, and none of
    /// them should rewrite the user's preferences.
    settings_dirty: bool,
    /// Which row the settings dialog has under its cursor.
    pub settings_row: usize,
    pub sort_by: SortBy,
    /// One cursor per panel, indexed by `Focus::index`. A single shared cursor
    /// let scrolling one card move another card's view — the panels have
    /// independent lengths and independent scroll positions. Read through
    /// `selected()`, which clamps against the focused panel's current length,
    /// so a panel that shrinks under its own cursor needs no separate fixup.
    selected: [usize; Focus::ALL.len()],
    /// A newer release, once the update check has found one. `None` until
    /// then, and `None` forever when the check is off or the network is not
    /// there — the dashboard never waits on it.
    pub update: Option<crate::update::Update>,
    pub pending_kill: Option<(i32, String)>,
    /// The open action waiting on "which port?". Set only when a row offers
    /// more than one browsable port — see `PortPick`.
    pub pending_port_pick: Option<PortPick>,
    /// A URL the `o` key asked to open, waiting for the event loop to drain it
    /// with `take_open_request`. Spawning the browser here instead would put a
    /// child process behind every keypress — including in tests, which press
    /// keys by the hundred. `App` decides *what* to open; `main` does the
    /// opening.
    open_request: Option<String>,
    /// A session the `o` key asked to jump to: its pid, and its tty if it has
    /// one. Drained by the event loop, which does the work on its own thread
    /// — finding the host reads the process table, and the AppleScript path
    /// can hang for minutes on a permission prompt.
    focus_request: Option<(i32, Option<String>)>,
    pub message: Option<String>,
    /// The burst's inner-ring rotation in degrees, wrapped to `0.0..360.0`.
    /// The outer ring is its negation, so one accumulator drives both. Thermal:
    /// the hotter the machine, the faster it turns.
    pub fan_angle_deg: f32,
    pub should_quit: bool,
    pub dirty: bool,
}

impl App {
    pub fn new(no_fan: bool) -> Self {
        let mut app = Self {
            statics: SystemStatic::read(),
            fast: None,
            medium: None,
            slow: None,
            cpu_hist: History::new(60),
            temp_hist: History::new(60),
            power_hist: History::new(60),
            net_hist: History::new(60),
            focus: Focus::Processes,
            settings: Settings::default(),
            theme: Settings::default().theme(),
            settings_open: false,
            settings_dirty: false,
            settings_row: 0,
            sort_by: SortBy::Cpu,
            selected: [0; Focus::ALL.len()],
            update: None,
            pending_kill: None,
            pending_port_pick: None,
            open_request: None,
            focus_request: None,
            message: None,
            fan_angle_deg: 0.0,
            should_quit: false,
            dirty: true,
        };
        app.settings.fan = !no_fan;
        app
    }

    /// Whether the header animation is suppressed. A view of `settings.fan`
    /// rather than a field of its own: two sources of truth for one switch is
    /// how a `--no-fan` that stops working gets shipped.
    pub fn no_fan(&self) -> bool {
        !self.settings.fan
    }

    /// Rows the settings dialog offers.
    pub const SETTINGS_ROWS: usize = 4;

    /// Re-resolve the palette after a setting changes. Cheap — `Theme` is
    /// eleven `Copy` colours — and doing it here rather than per-colour keeps
    /// the render path reading one value.
    pub fn apply_settings(&mut self) {
        self.theme = self.settings.theme();
    }

    /// Change the setting under the cursor. Every row cycles rather than
    /// having separate "next"/"previous" keys — with two to five options each,
    /// a second key would be ceremony.
    fn cycle_setting(&mut self) {
        match self.settings_row {
            0 => self.settings.palette = self.settings.palette.next(),
            1 => self.settings.accent = self.settings.accent.next(),
            // Inert when the palette forbids it, rather than silently
            // storing a value the dialog then contradicts.
            2 if self.settings.terminal_bg_available() => {
                self.settings.terminal_bg = !self.settings.terminal_bg;
            }
            2 => return,
            _ => self.settings.fan = !self.settings.fan,
        }
        self.apply_settings();
        self.settings_dirty = true;
    }

    /// Take the pending save, if any. The event loop calls this after every
    /// key and writes the config file.
    pub fn take_settings_save(&mut self) -> Option<Settings> {
        self.settings_dirty.then(|| {
            self.settings_dirty = false;
            self.settings
        })
    }

    /// Adopt the stored settings, then let the command line override them.
    ///
    /// Precedence is flag beats file beats default, and a flag deliberately
    /// does *not* rewrite the file: `--no-fan` for one run should not silently
    /// turn the fan off forever.
    pub fn load_settings(&mut self, no_fan_flag: bool) {
        self.settings = Settings::load();
        if no_fan_flag {
            self.settings.fan = false;
        }
        self.apply_settings();
        self.settings_dirty = false;
    }

    /// A pre-populated `App` for render tests and manual UI inspection: one
    /// `FastSnap` (a few processes), one `MediumSnap` (all
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
            total_cpu: 41.0,
            processes: vec![
                ProcInfo { pid: 101, name: "kernel_task".into(), cpu: 12.5, mem: 512_000 },
                ProcInfo { pid: 202, name: "WindowServer".into(), cpu: 8.3, mem: 256_000 },
                ProcInfo { pid: 303, name: "whirr".into(), cpu: 2.1, mem: 32_000 },
                ProcInfo { pid: 503, name: "claude".into(), cpu: 12.4, mem: 400_000 },
                ProcInfo { pid: 601, name: "claude".into(), cpu: 8.1, mem: 300_000 },
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
            // One session per state the card can show, so the demo exercises
            // every branch of the renderer rather than four idle rows.
            sessions: vec![
                ClaudeSession { pid: 601, project: "axterio".into(), title: None, jumpable: true, tty: Some("ttys020".into()),
                    state: SessionState { activity: Activity::Busy, subagents: 2, warn: false } },
                ClaudeSession { pid: 602, project: "axterio".into(), title: None, jumpable: true, tty: Some("ttys021".into()),
                    state: SessionState { activity: Activity::Loop { wakes_in: Duration::from_secs(260) }, subagents: 0, warn: true } },
                ClaudeSession { pid: 603, project: "whirr".into(), title: Some("✳ Fix the port picker".into()), jumpable: true, tty: Some("ttys004".into()),
                    state: SessionState { activity: Activity::Idle { since: Duration::from_secs(90).into() }, subagents: 0, warn: false } },
                ClaudeSession { pid: 604, project: "eye-claudius".into(), title: None, jumpable: false, tty: None,
                    state: SessionState { activity: Activity::BgJob, subagents: 0, warn: true } },
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
            Snapshot::Update(u) => self.update = Some(u),
            Snapshot::Notice(text) => {
                self.message = Some(text);
                self.dirty = true;
            }
        }
        self.dirty = true;
    }

    fn focused_len(&self) -> usize {
        match self.focus {
            Focus::Processes => self.processes().len(),
            Focus::Localhost => self.localhost_rows().len(),
            Focus::Sessions => self.sessions().len(),
            Focus::Others => self.other_rows().len(),
        }
    }

    /// The focused panel's cursor, clamped to its current length. Clamping on
    /// read rather than on every ingest is what lets a panel shrink and regrow
    /// without a fixup pass — and means no caller can observe an out-of-range
    /// cursor in the first place.
    pub fn selected(&self) -> usize {
        self.selected[self.focus.index()].min(self.focused_len().saturating_sub(1))
    }

    /// Move the focused panel's cursor. Other panels keep theirs.
    pub fn select(&mut self, i: usize) {
        self.selected[self.focus.index()] = i;
        self.dirty = true;
    }

    /// Every process the fast tick sampled, in sort order.
    ///
    /// This used to truncate to a constant 10, which had nothing to do with
    /// how much room the panel actually had — in the three-card body the table
    /// is handed all the leftover height, so a tall terminal drew ten rows
    /// above a stack of blank ones. How many fit is a rendering question, and
    /// `ui::processes` already answers it by windowing this slice against its
    /// own height.
    pub fn processes(&self) -> &[ProcInfo] {
        self.fast.as_ref().map_or(&[], |f| f.processes.as_slice())
    }

    /// Live CPU for an arbitrary pid — the ports card joins claude rows against
    /// the fast tick. Unlike `processes`, this searches the full list.
    pub fn cpu_of(&self, pid: i32) -> Option<f32> {
        self.fast.as_ref()?.processes.iter().find(|p| p.pid == pid).map(|p| p.cpu)
    }

    /// What a session is doing, for a row that knows only its pid.
    ///
    /// The grouped ports card carries Claude sessions too, at widths where
    /// they get no card of their own, and its rows are port-sourced — so the
    /// state has to be joined back on by pid rather than carried with them.
    pub fn session_state(&self, pid: i32) -> Option<&SessionState> {
        self.sessions().iter().find(|s| s.pid == pid).map(|s| &s.state)
    }

    /// Port rows belonging to the localhost card.
    pub fn localhost_rows(&self) -> Vec<&PortRow> {
        self.rows_in(PortGroup::Localhost)
    }

    /// Port rows belonging to the others card.
    pub fn other_rows(&self) -> Vec<&PortRow> {
        self.rows_in(PortGroup::Other)
    }

    fn rows_in(&self, g: PortGroup) -> Vec<&PortRow> {
        self.slow.as_ref().map(|s| s.rows.iter().filter(|r| r.group == g).collect()).unwrap_or_default()
    }

    pub fn sessions(&self) -> &[ClaudeSession] {
        self.slow.as_ref().map_or(&[], |s| &s.sessions)
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

        // Dialogs are checked before anything else and always return, so no
        // key can act on the dashboard behind one — and no second dialog can
        // stack on top of the first.
        if self.settings_open {
            match key.code {
                KeyCode::Up => self.settings_row = self.settings_row.saturating_sub(1),
                KeyCode::Down => {
                    self.settings_row = (self.settings_row + 1).min(Self::SETTINGS_ROWS - 1);
                }
                // Every row cycles, so left, right and Enter all mean "next
                // value" — there is nothing to go back to that going forward
                // does not reach.
                KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                    self.cycle_setting();
                }
                KeyCode::Esc | KeyCode::Char('s') => self.settings_open = false,
                _ => {}
            }
            return;
        }

        if let Some(pick) = self.pending_port_pick.clone() {
            match key.code {
                // '1' addresses the first entry, so the offset is one less.
                KeyCode::Char(c @ '1'..='9') => {
                    let i = c as usize - '1' as usize;
                    if let Some(&port) = pick.ports.get(i) {
                        self.open_request = Some(Self::localhost_url(port));
                        self.pending_port_pick = None;
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => self.pending_port_pick = None,
                _ => {}
            }
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
                KeyCode::Char('n') | KeyCode::Esc => self.pending_kill = None,
                // Every other key is ignored rather than treated as a cancel.
                // Cancelling on anything-but-y meant an arrow or a Tab
                // dismissed the question silently, and the next `k` read as
                // having done nothing. A dialog answers the keys it offers.
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('s') => {
                self.settings_open = true;
                self.settings_row = 0;
            }
            // Each panel keeps its own cursor across a Tab, so returning to a
            // card puts you back where you left it.
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::Up => self.select(self.selected().saturating_sub(1)),
            KeyCode::Down => {
                self.select((self.selected() + 1).min(self.focused_len().saturating_sub(1)));
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
                    if let Some(p) = self.processes().get(self.selected()) {
                        self.pending_kill = Some((p.pid, p.name.clone()));
                    }
                }
                // Only dev servers are killable. Sessions and system agents are
                // deliberately not one keypress from termination.
                Focus::Localhost => {
                    if let Some(r) = self.localhost_rows().get(self.selected()) {
                        let n = r.ports.len();
                        let ports = if n == 1 { "port" } else { "ports" };
                        self.pending_kill = Some((r.pid, format!("{} ({n} {ports})", r.label)));
                    }
                }
                Focus::Sessions | Focus::Others => {}
            },
            // Opening is the read-only counterpart to `k`, so it lives on the
            // same card and needs no confirmation: a browser tab is undone by
            // closing it, a SIGTERM is not.
            // Gated on focus, not just on there being a localhost row to find:
            // `selected()` is the *focused* panel's cursor, so an ungated arm
            // would index the localhost list with the process cursor and open
            // whatever happened to sit at that offset.
            // The sessions card has no URL, but it does have somewhere to
            // go: the terminal the session is running in.
            KeyCode::Char('o') if matches!(self.focus, Focus::Sessions) => {
                if let Some(s) = self.sessions().get(self.selected()).filter(|s| s.jumpable) {
                    self.focus_request = Some((s.pid, s.tty.clone()));
                }
            }
            KeyCode::Char('o') if matches!(self.focus, Focus::Localhost) => {
                // Scoped: `localhost_rows` borrows self, and both arms below
                // write to it.
                let row = {
                    let rows = self.localhost_rows();
                    rows.get(self.selected())
                        .map(|r| (r.label.clone(), ports::browsable(&r.ports)))
                };
                if let Some((label, ports)) = row {
                    match ports.as_slice() {
                        [] => {}
                        // One candidate is not a choice — asking would make
                        // the common case slower for nothing.
                        [port] => self.open_request = Some(Self::localhost_url(*port)),
                        _ => {
                            let ports = ports.into_iter().take(MAX_PICKABLE_PORTS).collect();
                            self.pending_port_pick = Some(PortPick { label, ports });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Everything whirr opens is a dev server on this machine, so the scheme
    /// and host are never in question — only the port ever was.
    fn localhost_url(port: u16) -> String {
        format!("http://localhost:{port}")
    }

    /// Whether the focused session's terminal can be put in front. Drives
    /// both the `o` key and whether the footer offers it.
    pub fn selected_session_is_jumpable(&self) -> bool {
        self.sessions().get(self.selected()).is_some_and(|s| s.jumpable)
    }

    /// Take the pending jump-to-session request, if any.
    pub fn take_focus_request(&mut self) -> Option<(i32, Option<String>)> {
        self.focus_request.take()
    }

    /// Take the pending open request, if any. The event loop calls this after
    /// every key and hands the URL to `open(1)`.
    pub fn take_open_request(&mut self) -> Option<String> {
        self.open_request.take()
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
    use crate::settings::Palette;
    use crossterm::event::{KeyCode, KeyEvent};

    fn key(c: char) -> KeyEvent {
        KeyEvent::from(KeyCode::Char(c))
    }

    fn demo_fast() -> FastSnap {
        FastSnap {
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
        assert_eq!(a.processes()[0].name, "hog");
        a.on_key(key('m'));
        assert_eq!(a.processes()[0].name, "ram");
        a.on_key(key('c'));
        assert_eq!(a.processes()[0].name, "hog");
    }

    #[test]
    fn selection_clamps() {
        let mut a = app_with_procs();
        a.on_key(KeyEvent::from(KeyCode::Down));
        a.on_key(KeyEvent::from(KeyCode::Down));
        a.on_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(a.selected(), 1); // only 2 processes
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
    fn s_opens_settings_and_esc_closes_it() {
        let mut a = App::demo();
        press(&mut a, 's');
        assert!(a.settings_open, "s should open the dialog");
        a.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(!a.settings_open, "Esc should close it");
    }

    #[test]
    fn changing_a_setting_takes_effect_immediately() {
        // The point of a dialog over a config file: you see the change while
        // you are choosing it.
        let mut a = App::demo();
        let before = a.theme;
        press(&mut a, 's');
        a.on_key(KeyEvent::from(KeyCode::Right)); // palette: dark -> light
        assert_ne!(a.theme, before, "the palette change should already be visible");
        assert_eq!(a.theme, a.settings.theme(), "the live theme must follow the settings");
    }

    #[test]
    fn the_settings_dialog_swallows_keys_that_would_otherwise_act() {
        // Same discipline as the other two dialogs: k must not raise a kill
        // confirmation underneath the settings.
        let mut a = App::demo();
        press(&mut a, 's');
        press(&mut a, 'k');
        press(&mut a, 'q');
        assert!(a.pending_kill.is_none(), "no dialog should stack on settings");
        assert!(!a.should_quit, "q is inert while a dialog is open");
        assert!(a.settings_open);
    }

    #[test]
    fn the_cursor_moves_between_rows_and_stops_at_the_ends() {
        let mut a = App::demo();
        press(&mut a, 's');
        assert_eq!(a.settings_row, 0);
        a.on_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(a.settings_row, 0, "must not run off the top");
        for _ in 0..20 {
            a.on_key(KeyEvent::from(KeyCode::Down));
        }
        assert_eq!(a.settings_row, App::SETTINGS_ROWS - 1, "must not run off the bottom");
    }

    #[test]
    fn changing_a_setting_asks_the_event_loop_to_save_it_once() {
        let mut a = App::demo();
        assert_eq!(a.take_settings_save(), None, "nothing to save before anything changes");
        press(&mut a, 's');
        a.on_key(KeyEvent::from(KeyCode::Right));
        let saved = a.take_settings_save().expect("a change should ask to be saved");
        assert_eq!(saved.palette, Palette::Light, "what is saved is what was chosen");
        assert_eq!(a.take_settings_save(), None, "and only once");
    }

    #[test]
    fn merely_opening_and_closing_settings_saves_nothing() {
        // Otherwise every accidental `s` rewrites the file with no change in
        // it, and the file's mtime stops meaning anything.
        let mut a = App::demo();
        press(&mut a, 's');
        a.on_key(KeyEvent::from(KeyCode::Down));
        a.on_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(a.take_settings_save(), None);
    }

    #[test]
    fn the_background_row_is_inert_under_the_light_palette() {
        // Light means dark text; an unpainted frame would put it on whatever
        // the terminal's background is. The key does nothing rather than
        // storing a value the dialog then contradicts.
        let mut a = App::demo();
        press(&mut a, 's');
        a.on_key(KeyEvent::from(KeyCode::Right)); // theme -> light
        let _ = a.take_settings_save();
        a.on_key(KeyEvent::from(KeyCode::Down));
        a.on_key(KeyEvent::from(KeyCode::Down)); // -> background
        a.on_key(KeyEvent::from(KeyCode::Right));
        assert!(!a.settings.terminal_bg, "the choice must not be taken");
        assert_eq!(a.take_settings_save(), None, "and nothing to save");
        assert!(a.theme.paint_bg, "the frame is painted regardless");
    }

    #[test]
    fn the_fan_setting_drives_the_animation() {
        let mut a = App::demo();
        assert!(!a.no_fan(), "the fan runs by default");
        a.settings.fan = false;
        assert!(a.no_fan(), "turning the setting off stops the animation");
    }

    #[test]
    fn esc_cancels_a_pending_kill() {
        let mut a = app_with_procs();
        a.on_key(key('k'));
        a.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(a.pending_kill.is_none(), "Esc is the other obvious way out of a dialog");
    }

    #[test]
    fn keys_the_dialog_does_not_offer_leave_it_standing() {
        // It used to cancel on *any* key but `y`, so an arrow or a Tab
        // dismissed the question silently and the next `k` looked like it had
        // done nothing. A dialog should answer only the keys it advertises.
        for k in [KeyCode::Down, KeyCode::Up, KeyCode::Tab, KeyCode::Char('c')] {
            let mut a = app_with_procs();
            a.on_key(key('k'));
            a.on_key(KeyEvent::from(k));
            assert!(a.pending_kill.is_some(), "{k:?} should not dismiss the dialog");
        }
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
    fn the_process_table_offers_every_sampled_process() {
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
        assert_eq!(a.processes().len(), 30, "the table offers every sampled process");
        // The cursor clamps to the list, not to a display constant: how many
        // rows are on screen is the renderer's business, and it windows this
        // slice against its own height.
        for _ in 0..40 {
            a.on_key(KeyEvent::from(KeyCode::Down));
        }
        assert_eq!(a.selected(), 29);
    }

    #[test]
    fn cpu_of_finds_a_pid_anywhere_in_the_sample() {
        // cpu_of is how the ports and sessions cards join their rows against
        // the fast tick, so it searches by pid rather than by position — the
        // process it wants is routinely nowhere near the top of the table.
        let mut a = App::new(false);
        let mut f = demo_fast();
        f.processes = (0..15)
            .map(|i| ProcInfo { pid: 900 + i, name: format!("p{i}"), cpu: i as f32, mem: 0 })
            .collect();
        a.ingest(Snapshot::Fast(f));
        assert_eq!(a.cpu_of(914), Some(14.0), "the last-sorted process is still findable");
        assert_eq!(a.cpu_of(1), None);
    }

    fn press(a: &mut App, c: char) {
        a.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }

    #[test]
    fn kill_works_on_a_localhost_port_row() {
        let mut a = App::demo();
        a.focus = Focus::Localhost;
        a.select(0); // demo()'s first localhost row is glassbook-frontend
        press(&mut a, 'k');
        let (pid, label) = a.pending_kill.clone().expect("localhost row must be killable");
        assert_eq!(pid, 501);
        assert!(label.contains("glassbook-frontend"), "dialog should name the process: {label}");
        assert!(label.contains('3'), "dialog should say how many ports die with it: {label}");
    }

    /// `App::demo()` on the localhost card with `row` selected.
    fn demo_on_localhost(row: usize) -> App {
        let mut a = App::demo();
        a.focus = Focus::Localhost;
        a.select(row);
        a
    }

    #[test]
    fn o_on_a_row_with_one_candidate_opens_it_without_asking() {
        // axterio listens on :3000 and nothing else. Asking here would make
        // the common case slower for no benefit.
        let mut a = demo_on_localhost(1);
        press(&mut a, 'o');
        assert_eq!(a.take_open_request().as_deref(), Some("http://localhost:3000"));
        assert!(a.pending_port_pick.is_none(), "one candidate needs no dialog");
    }

    #[test]
    fn o_on_a_row_with_several_candidates_asks_instead_of_guessing() {
        // glassbook-frontend listens on 4206, 6006 and 63643. 63643 is
        // ephemeral, so the real choice is 4206 vs 6006 — and guessing the
        // lower one is exactly the Storybook bug this replaces.
        let mut a = demo_on_localhost(0);
        press(&mut a, 'o');
        assert_eq!(a.take_open_request(), None, "it must not guess");
        let pick = a.pending_port_pick.clone().expect("a choice should be pending");
        assert_eq!(pick.ports, vec![4206, 6006]);
        assert_eq!(pick.label, "glassbook-frontend");
    }

    #[test]
    fn picking_the_second_port_opens_that_one() {
        let mut a = demo_on_localhost(0);
        press(&mut a, 'o');
        press(&mut a, '2');
        assert_eq!(a.take_open_request().as_deref(), Some("http://localhost:6006"));
        assert!(a.pending_port_pick.is_none(), "the dialog should close once answered");
    }

    #[test]
    fn esc_and_n_cancel_the_port_pick() {
        for cancel in [KeyCode::Esc, KeyCode::Char('n')] {
            let mut a = demo_on_localhost(0);
            press(&mut a, 'o');
            a.on_key(KeyEvent::from(cancel));
            assert!(a.pending_port_pick.is_none(), "{cancel:?} should close the picker");
            assert_eq!(a.take_open_request(), None, "cancelling must not open anything");
        }
    }

    #[test]
    fn a_digit_past_the_end_of_the_list_leaves_the_picker_standing() {
        // Two candidates, so 3 addresses nothing. Same discipline as the kill
        // dialog: answer the keys offered, ignore the rest.
        let mut a = demo_on_localhost(0);
        press(&mut a, 'o');
        press(&mut a, '3');
        assert!(a.pending_port_pick.is_some(), "3 addresses no entry");
        assert_eq!(a.take_open_request(), None);
    }

    #[test]
    fn the_port_picker_swallows_keys_that_would_otherwise_act() {
        // k must not raise a kill dialog on top of the picker.
        let mut a = demo_on_localhost(0);
        press(&mut a, 'o');
        press(&mut a, 'k');
        assert!(a.pending_kill.is_none(), "a second dialog must not stack on the first");
        assert!(a.pending_port_pick.is_some());
    }

    #[test]
    fn an_open_request_is_taken_once() {
        // The event loop drains this to spawn `open`. If it survived the
        // take, every later keypress would reopen the same tab.
        let mut a = App::demo();
        a.focus = Focus::Localhost;
        a.select(1);
        press(&mut a, 'o');
        assert_eq!(a.take_open_request().as_deref(), Some("http://localhost:3000"));
        assert_eq!(a.take_open_request(), None, "a drained request must not come back");
    }

    #[test]
    fn o_is_inert_on_every_card_but_localhost() {
        // Same rule as `k`: a key that means nothing on a card must do
        // nothing there, rather than open a browser at a system daemon's port.
        for focus in [Focus::Processes, Focus::Sessions, Focus::Others] {
            let mut a = App::demo();
            a.focus = focus;
            a.select(0);
            press(&mut a, 'o');
            assert_eq!(a.take_open_request(), None, "{focus:?} has no URL to open");
        }
    }

    #[test]
    fn o_on_a_session_asks_to_jump_to_its_terminal() {
        let mut a = App::demo();
        a.focus = Focus::Sessions;
        a.select(0); // axterio on ttys020
        press(&mut a, 'o');
        assert_eq!(a.take_focus_request(), Some((601, Some("ttys020".into()))));
        assert_eq!(a.take_focus_request(), None, "the request is taken once");
        assert_eq!(a.take_open_request(), None, "a session is not a URL");
    }

    #[test]
    fn a_session_the_host_cannot_reach_does_not_offer_the_key() {
        // Merely activating the application is not a jump: whirr is often
        // running inside that same app, so it is indistinguishable from a key
        // that does nothing — which is exactly how it was reported.
        let mut a = App::demo();
        a.focus = Focus::Sessions;
        a.select(3); // eye-claudius: no tty, so no surface can match
        assert!(!a.selected_session_is_jumpable());
        press(&mut a, 'o');
        assert_eq!(a.take_focus_request(), None, "the key must be inert, not hopeful");
    }

    #[test]
    fn o_is_still_inert_where_there_is_nowhere_to_go() {
        for focus in [Focus::Processes, Focus::Others] {
            let mut a = App::demo();
            a.focus = focus;
            a.select(0);
            press(&mut a, 'o');
            assert_eq!(a.take_focus_request(), None, "{focus:?}");
            assert_eq!(a.take_open_request(), None, "{focus:?}");
        }
    }

    #[test]
    fn kill_is_inert_on_claude_and_other_rows() {
        for focus in [Focus::Sessions, Focus::Others] {
            let mut a = App::demo();
            a.focus = focus;
            a.select(0);
            press(&mut a, 'k');
            assert!(
                a.pending_kill.is_none(),
                "{focus:?} must not be killable — a stray k must not end a session or a system agent"
            );
        }
    }

    #[test]
    fn tab_cycles_all_four_panels_and_wraps() {
        let mut a = App::demo();
        let seen = |a: &App| format!("{:?}", a.focus);
        let mut order = vec![seen(&a)];
        for _ in 0..4 {
            a.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
            order.push(seen(&a));
        }
        assert_eq!(
            order,
            vec!["Processes", "Localhost", "Sessions", "Others", "Processes"],
            "Tab must visit every focusable panel and wrap"
        );
    }

    #[test]
    fn each_card_reports_its_own_row_count() {
        let a = App::demo();
        // demo(): 2 localhost rows, 1 other row, 4 sessions.
        assert_eq!(a.localhost_rows().len(), 2);
        assert_eq!(a.other_rows().len(), 1);
        assert_eq!(a.sessions().len(), 4);
    }

    #[test]
    fn each_panel_keeps_its_own_cursor() {
        // One shared cursor let moving the selection in one card scroll and
        // re-highlight another. Panels have independent lengths and
        // independent scroll positions, so they get independent cursors.
        let mut a = App::demo();
        a.on_key(KeyEvent::from(KeyCode::Down)); // Processes -> 1
        a.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // -> Localhost
        assert_eq!(a.selected(), 0, "a freshly focused panel starts at its own cursor");
        a.on_key(KeyEvent::from(KeyCode::Down)); // Localhost -> 1

        a.focus = Focus::Processes;
        assert_eq!(a.selected(), 1, "the process cursor must survive another panel moving");
        a.focus = Focus::Sessions;
        assert_eq!(a.selected(), 0, "sessions was never moved");
    }

    #[test]
    fn a_cursor_is_clamped_to_its_own_panels_length() {
        // Panels are different lengths; a cursor parked past the end of a
        // shorter one must read as that panel's last row, not out of range.
        let mut a = App::demo(); // 5 processes, 2 localhost rows, 4 sessions
        for _ in 0..9 {
            a.on_key(KeyEvent::from(KeyCode::Down));
        }
        assert_eq!(a.selected(), 4, "Down stops at the last process");
        a.focus = Focus::Localhost;
        a.select(9);
        assert_eq!(a.selected(), 1, "clamped to the 2-row localhost card");
        a.focus = Focus::Processes;
        assert_eq!(a.selected(), 4, "clamping one panel must not disturb another");
    }
}
