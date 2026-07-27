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

fn has_braille(s: &str) -> bool {
    s.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
}

/// Row/col grid of a render, for asserting on exact panel boundaries rather
/// than "some text appears somewhere in the flattened buffer".
fn draw_grid(w: u16, h: u16) -> Vec<String> {
    let flat = draw_at(w, h);
    let cells: Vec<char> = flat.chars().collect();
    cells.chunks(w as usize).map(|row| row.iter().collect()).collect()
}

/// Row index of the first line containing `needle` — for a panel_block,
/// that's the row its title (and top border) is drawn on.
fn row_of(lines: &[String], needle: &str) -> usize {
    lines.iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("{needle:?} not found"))
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

// Regression test for the 80x24 body split: with show_ports/show_network
// true and a compact-tier header (3) + gauges (10), the body gets 11 rows.
// Paired against Min(4), Max(13) hands the process table up to its cap but
// not below the point where ports would starve — at body height 11 that
// solves to (7, 4): the process table shrinks to 7 rows and ports gets
// exactly its Min(4) floor. This also pins that the header/gauges rows
// (resolved by the outer `Layout::split` before this body split ever runs)
// stay their fixed sizes regardless of what the body split below does.
#[test]
fn stock_80x24_fits_without_starving_header_or_ports() {
    let lines = draw_grid(80, 24);
    assert_eq!(lines.len(), 24);

    // Header is 3 rows, gauges 10: the first gauge card's title lands
    // exactly on row 3, unaffected by anything the body split below decides.
    assert_eq!(row_of(&lines, "CPU"), 3, "gauges row wasn't given its fixed height");

    // Body starts at row 13 (3 + 10). The process table's Max(13) cap can't
    // be met at body height 11, so it shrinks to 7 rows; Ports starts right
    // after and gets its Min(4) floor, ending exactly on the last row.
    let processes_row = row_of(&lines, "Processes");
    assert_eq!(processes_row, 13, "process table didn't start where the body begins");
    let ports_row = row_of(&lines, "Ports");
    assert_eq!(ports_row - processes_row, 7, "process table should have shrunk to 7 rows, not its full 13-row cap");
    assert_eq!(ports_row, 20, "ports card should start right after the shrunk process table");
    // Ports keeps its Min(4) floor: 4 rows (20..=23) reaching exactly the
    // last row of the 24-row terminal, rather than being squeezed further.
    assert_eq!(lines.len() - 1, ports_row + 3, "ports card should occupy exactly its Min(4) floor");
}

// At a generous body height the process table should claim its full
// Max(13) cap rather than something smaller, and ports should get the
// remainder (well above its Min(4) floor) — the mirror case of the 80x24
// test above, confirming Max(13) behaves like a real cap, not a shrink-to-0.
#[test]
fn large_size_gives_process_table_its_full_cap() {
    let lines = draw_grid(160, 45);
    assert_eq!(lines.len(), 45);

    // Full tier: header 9 rows, gauges 12 rows, so the body (and the
    // process table inside it) starts at row 21.
    let processes_row = row_of(&lines, "Processes");
    assert_eq!(processes_row, 21, "process table didn't start where the full-tier body begins");

    let ports_row = row_of(&lines, "Ports");
    assert_eq!(ports_row - processes_row, 13, "process table should claim its full Max(13) cap when there's room");
    // Body height here is 24 (45 - 9 - 12), so ports gets 24 - 13 = 11 rows,
    // comfortably above its Min(4) floor.
    assert_eq!(44 - ports_row + 1, 11, "ports should get the remaining 11 rows, not just its Min(4) floor");
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
fn full_tier_shows_hero_font_and_burst_fan() {
    let c = draw_at(160, 45);
    assert!(has_braille(&c), "burst fan missing");
    assert!(c.contains("█ ▄ █"), "4-row logo W missing");
    assert!(c.contains("▄  █"), "cpu hero '4' glyph missing"); // total_cpu 41 → "41%"
    assert!(c.contains("█  ▄▀"), "hero '%' glyph missing");
}

#[test]
fn compact_tier_keeps_old_visuals() {
    let c = draw_at(80, 24);
    assert!(!has_braille(&c), "burst fan must not render at 80x24");
    assert!(!c.contains("█ ▄ █"), "4-row logo must not render at 80x24");
    assert!(c.contains("88.0°C"), "compact temp readout missing");
}

#[test]
fn tier_boundary_is_exactly_120x30() {
    assert!(has_braille(&draw_at(120, 30)), "120x30 must be full tier");
    assert!(!has_braille(&draw_at(119, 30)), "119x30 must be compact");
    assert!(!has_braille(&draw_at(120, 29)), "120x29 must be compact");
}
