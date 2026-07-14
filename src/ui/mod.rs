// font::big_text and some of theme:: are consumed by panels arriving in
// Tasks 15-20; header/cpu/draw are wired in now, so only those stay allowed.
#[allow(dead_code)]
pub mod font;
pub mod cpu;
pub mod header;
pub mod memory;
pub mod network;
pub mod power;
pub mod processes;
pub mod temp;
#[allow(dead_code)]
pub mod theme;

use ratatui::prelude::*;

use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    // Interim layout: finalized in Task 21. Rows 1[1..4] and 2-3 are filled
    // in by Tasks 15-20; for now only the CPU panel (gauges[0]) renders.
    let rows = Layout::vertical([
        Constraint::Length(3),  // header
        Constraint::Length(10), // gauges row: cpu/temp/power/memory
        Constraint::Min(8),     // processes + network
        Constraint::Length(4),  // ports
    ])
    .split(f.area());
    header::render(f, rows[0], app);
    let gauges = Layout::horizontal([Constraint::Ratio(1, 4); 4]).split(rows[1]);
    cpu::render(f, gauges[0], app);
    temp::render(f, gauges[1], app);
    power::render(f, gauges[2], app);
    memory::render(f, gauges[3], app);
    let mid = Layout::horizontal([Constraint::Ratio(3, 5), Constraint::Ratio(2, 5)]).split(rows[2]);
    processes::render(f, mid[0], app);
    network::render(f, mid[1], app);
}
