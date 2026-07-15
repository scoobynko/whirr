pub mod font;
pub mod cpu;
pub mod header;
pub mod memory;
pub mod network;
pub mod ports;
pub mod power;
pub mod processes;
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
/// left column the process table is a `Max`, not a `Length`, capped at
/// `MAX_VISIBLE_PROCS` rows (+ footer + borders): it only claims that much
/// space when it's available. At heights below the point where header +
/// gauges + full process table + ports card (`Min(4)`) all fit, the process
/// table shrinks gracefully (fewer visible rows) rather than starving the
/// other panels — ratatui's solver holds `Min` constraints firm ahead of
/// `Length`, so an over-committed `Length` here would squeeze header/gauges/
/// ports arbitrarily at common sizes like 80x24. The ports card grows with
/// available height once the process table has its room.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let show_ports = area.height >= 20;
    let show_network = area.height >= 16;
    let show_power = area.width >= 70;
    let show_temp = area.width >= 50;

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(10),
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
