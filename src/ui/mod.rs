pub mod font;
pub mod burst;
pub mod cpu;
pub mod header;
pub mod memory;
pub mod network;
pub mod ports;
pub mod power;
pub mod processes;
pub mod spark;
pub mod temp;
pub mod theme;

use ratatui::prelude::*;

use crate::app::{App, MAX_VISIBLE_PROCS};

/// Final responsive layout. Panels drop out in priority order as space gets
/// tight: ports first, then network, then power, then temp — processes and
/// CPU always survive. The gauges row splits into as many equal columns as
/// there are visible gauge panels (cpu + temp? + power? + memory).
///
/// The body below the gauges splits into a left column (processes stacked
/// over ports) and a right column (network at full body height). Within the
/// left column the process table is capped at `MAX_VISIBLE_PROCS` rows (+
/// footer + borders) via `Max`, paired with a `Min(4)` floor for the ports
/// card below it: the process table claims up to its cap and ports gets
/// whatever's left, but never below its floor — so at tight body heights the
/// process table shrinks (fewer visible rows) while ports keeps its minimum,
/// and once there's slack beyond both the process table takes its full cap
/// and ports grows into the rest. (`Length(13)` paired with the same
/// `Min(4)` produces an identical split in ratatui 0.29 at every body height
/// tested — `Max` is used here because it documents the "cap, not a fixed
/// size" intent, not because it behaves differently from `Length` in this
/// pairing.) Either way, this constraint can only ever move space between
/// the process table and ports: the header and gauges rows are resolved by
/// the outer `Layout::split` above, before `render_left_column` is ever
/// called, so nothing inside it can squeeze them.
///
/// Full visual tier (>=120x30): padded header with the braille burst fan
/// (9 rows) and hero-number gauge cards (12 rows). Compact tier: standard
/// header (3 rows) and compact gauges (10 rows).
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let show_ports = area.height >= 20;
    let show_network = area.height >= 16;
    let show_power = area.width >= 70;
    let show_temp = area.width >= 50;
    // Full visual tier: padded header with the braille burst fan, hero-number
    // gauge cards. Needs width for the ~27-col hero strings (4 cards x 30
    // cols) and height for header 9 + gauges 12 + a useful body.
    let full = area.height >= 30 && area.width >= 120;

    let chunks = Layout::vertical([
        Constraint::Length(if full { 9 } else { 3 }),
        Constraint::Length(if full { 12 } else { 10 }),
        Constraint::Min(6),
    ])
    .split(area);

    header::render(f, chunks[0], app);

    let n = 1 + usize::from(show_temp) + usize::from(show_power) + 1; // cpu + temp? + power? + memory
    let gauges = Layout::horizontal(vec![Constraint::Ratio(1, n as u32); n]).split(chunks[1]);
    let mut gi = 0;
    cpu::render(f, gauges[gi], app);
    gi += 1;
    if show_temp {
        temp::render(f, gauges[gi], app);
        gi += 1;
    }
    if show_power {
        power::render(f, gauges[gi], app);
        gi += 1;
    }
    memory::render(f, gauges[gi], app);

    let body = chunks[2];
    if show_network {
        let cols = Layout::horizontal([Constraint::Ratio(3, 5), Constraint::Ratio(2, 5)])
            .split(body);
        render_left_column(f, cols[0], app, show_ports);
        network::render(f, cols[1], app);
    } else {
        render_left_column(f, body, app, show_ports);
    }
}

/// Processes stacked over ports (when ports fit).
fn render_left_column(f: &mut Frame, area: Rect, app: &App, show_ports: bool) {
    if show_ports {
        // 10 process rows + 1 footer + 2 borders
        let procs_cap = MAX_VISIBLE_PROCS as u16 + 3;
        let rows = Layout::vertical([Constraint::Max(procs_cap), Constraint::Min(4)])
            .split(area);
        processes::render(f, rows[0], app);
        ports::render(f, rows[1], app);
    } else {
        processes::render(f, area, app);
    }
}
