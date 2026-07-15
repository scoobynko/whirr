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
/// Process table capped at `MAX_VISIBLE_PROCS` rows (+ footer + borders); the
/// middle row is a `Max`, not a `Length`, so it only claims that much space
/// when it's available. At heights below the point where header + gauges +
/// full process table + ports card (`Min(4)`) all fit, the middle row shrinks
/// gracefully (fewer visible process rows) rather than starving the other
/// panels — ratatui's solver holds `Min` constraints firm ahead of `Length`,
/// so an over-committed `Length` here would squeeze header/gauges/ports
/// arbitrarily at common sizes like 80x24. Ports card grows with available
/// height once the process table has its room.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let show_ports = area.height >= 20;
    let show_network = area.height >= 16;
    let show_power = area.width >= 70;
    let show_temp = area.width >= 50;

    // 10 process rows + 1 footer + 2 borders
    let middle = MAX_VISIBLE_PROCS as u16 + 3;

    let mut rows = vec![Constraint::Length(3), Constraint::Length(10)];
    if show_ports {
        rows.push(Constraint::Max(middle));
        rows.push(Constraint::Min(4));
    } else {
        rows.push(Constraint::Min(6));
    }
    let chunks = Layout::vertical(rows).split(area);

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

    if show_network {
        let mid = Layout::horizontal([Constraint::Ratio(3, 5), Constraint::Ratio(2, 5)])
            .split(chunks[2]);
        processes::render(f, mid[0], app);
        network::render(f, mid[1], app);
    } else {
        processes::render(f, chunks[2], app);
    }

    if show_ports {
        ports::render(f, chunks[3], app);
    }
}
