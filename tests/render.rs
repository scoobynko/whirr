use ratatui::backend::TestBackend;
use ratatui::Terminal;
use whirr::app::App;
use whirr::ui;

fn draw_at(w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    let app = App::demo();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    buf.content().iter().map(|c| c.symbol()).collect()
}

#[test]
fn renders_at_all_sizes_without_panic() {
    for (w, h) in [(200, 50), (120, 40), (80, 24), (60, 15), (20, 5)] {
        let content = draw_at(w, h);
        assert!(!content.is_empty(), "{w}x{h}");
    }
}

#[test]
fn full_size_shows_all_panels() {
    let c = draw_at(160, 45);
    for needle in ["CPU", "Temp", "Power", "Memory", "Processes", "Network", "Ports"] {
        assert!(c.contains(needle), "missing {needle}");
    }
    assert!(c.contains("(my-app)"), "port project badge missing");
}

#[test]
fn tiny_size_collapses_to_essentials() {
    let c = draw_at(48, 14);
    assert!(c.contains("Processes"));
    assert!(!c.contains("Ports"));
}

// Regression test for the 80x24 layout over-commit: at a stock terminal
// size, ports must stay visible (Min(4) floor) while the process table
// shrinks gracefully (Max(13) middle row) instead of the solver squeezing
// header/gauges arbitrarily to satisfy an over-committed Length(13).
#[test]
fn stock_80x24_fits_without_starving_header_or_ports() {
    let c = draw_at(80, 24);
    assert!(c.contains("Ports"), "ports card collapsed at 80x24");
    assert!(c.contains("Processes"), "process table missing at 80x24");
    // header facts line ("up <duration> · load <n>") must still render, i.e.
    // the header row wasn't squeezed to make room for the ports Min(4) floor.
    assert!(c.contains("up ") || c.contains("load "), "header facts missing at 80x24");
}

// At generous heights the middle (process table) row should claim its full
// cap (MAX_VISIBLE_PROCS rows + footer + borders) rather than the ports card
// eating into it — Max(13) should behave like Length(13) once there's slack.
#[test]
fn large_size_gives_process_table_its_full_cap() {
    let c = draw_at(160, 45);
    assert!(c.contains("Ports"));
    // App::demo() only seeds 3 processes, so we can't pin an 11th-row
    // absence; instead assert the demo processes and the ports project
    // badge both render, confirming neither panel starved the other.
    assert!(c.contains("(my-app)"), "port project badge missing at 160x45");
    assert!(c.contains("kernel_task"), "process rows missing at 160x45");
}

// The ports card lives in the LEFT column under the process table, not in a
// full-width bottom row: on the terminal row where the ports panel's title
// border is drawn, the rightmost cell must belong to the network panel's
// right border ('│'), not a ports corner ('╮'), which is what a full-width
// ports row would put there.
#[test]
fn ports_card_sits_under_processes_not_full_width() {
    let (w, h) = (160u16, 45u16);
    let flat = draw_at(w, h);
    let cells: Vec<char> = flat.chars().collect();
    let lines: Vec<String> = cells
        .chunks(w as usize)
        .map(|row| row.iter().collect())
        .collect();
    assert_eq!(lines.len(), h as usize);
    let ports_row = lines
        .iter()
        .position(|l| l.contains("Ports"))
        .expect("ports title row present");
    let last_char = lines[ports_row].chars().last().unwrap();
    assert_eq!(
        last_char, '│',
        "ports title row should end inside the network panel's right border, \
         got {last_char:?} (full-width ports row?)"
    );
}

#[test]
fn full_tier_shows_hero_font_and_housed_fan() {
    let c = draw_at(160, 45);
    assert!(c.contains("╭─────╮"), "housed fan missing");
    assert!(c.contains("█ ▄ █"), "4-row logo W missing");
    assert!(c.contains("▄  █"), "cpu hero '4' glyph missing"); // total_cpu 41 → "41%"
    assert!(c.contains("█  ▄▀"), "hero '%' glyph missing");
}

#[test]
fn compact_tier_keeps_old_visuals() {
    let c = draw_at(80, 24);
    assert!(!c.contains("╭─────╮"), "housed fan must not render at 80x24");
    assert!(!c.contains("█ ▄ █"), "4-row logo must not render at 80x24");
    assert!(c.contains("88.0°C"), "compact temp readout missing");
}

#[test]
fn tier_boundary_is_exactly_120x30() {
    assert!(draw_at(120, 30).contains("╭─────╮"), "120x30 must be full tier");
    assert!(!draw_at(119, 30).contains("╭─────╮"), "119x30 must be compact");
    assert!(!draw_at(120, 29).contains("╭─────╮"), "120x29 must be compact");
}
