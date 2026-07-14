// font::big_text and some of theme:: are consumed by panels arriving in
// Tasks 15-20; header/cpu/draw are wired in now, so only those stay allowed.
#[allow(dead_code)]
pub mod font;
pub mod cpu;
pub mod header;
pub mod memory;
pub mod network;
pub mod ports;
pub mod power;
pub mod processes;
pub mod temp;
#[allow(dead_code)]
pub mod theme;

use ratatui::prelude::*;

use crate::app::App;

/// Final responsive layout. Panels drop out in priority order as space gets
/// tight: ports first, then network, then power, then temp — processes and
/// CPU always survive. The gauges row splits into as many equal columns as
/// there are visible gauge panels (cpu + temp? + power? + memory).
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let show_ports = area.height >= 20;
    let show_network = area.height >= 16;
    let show_power = area.width >= 70;
    let show_temp = area.width >= 50;

    let mut rows = vec![Constraint::Length(3), Constraint::Length(10), Constraint::Min(6)];
    if show_ports {
        rows.push(Constraint::Length(4));
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
