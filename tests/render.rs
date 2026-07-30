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
    // At >=120 columns the body splits into three side-by-side cards
    // (localhost, claude sessions, others) instead of the single grouped
    // "Ports" card — see `three_cards_render_side_by_side_at_full_width`.
    for needle in [
        "CPU", "Temp", "Power", "Memory", "Processes", "Network",
        "localhost", "claude sessions", "others",
    ] {
        assert!(c.contains(needle), "missing {needle}");
    }
    assert!(c.contains("glassbook-frontend"), "port row missing");
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

// At >=120 columns, `three_cards` is always active (it only checks width),
// so the old single-Ports-card body split (Max(13)/Min(4)) can no longer
// coexist with the full-tier header/gauges (which also require width>=120)
// — that scenario the pre-three-cards suite exercised here no longer
// exists. What replaces it: the three-card row is `Length(6)` (4 content
// rows) when the body has room, but shrinks below that at tight heights
// (120x30 gets only 2 — see the three-card layout tests below). At a
// generous body height all four of `demo()`'s sessions should fit, proving
// the row actually reached its full 4 content rows rather than staying
// shrunk to the 120x30 case's 2.
#[test]
fn large_size_gives_the_three_card_row_its_full_height() {
    let c = draw_at(160, 45);
    for tty in ["ttys020", "ttys021", "ttys004"] {
        assert!(c.contains(tty), "missing {tty} — three-card row didn't reach its full height");
    }
    assert!(c.contains("eye-claudius"), "fourth demo session missing at large size");
}

// Below the 120-column three-card threshold, the ports card lives in the
// LEFT column under the process table, not in a full-width bottom row: on
// the terminal row where the ports panel's title border is drawn, the
// rightmost cell must belong to the network panel's right border ('│'),
// not a ports corner ('╮'), which is what a full-width ports row would put
// there.
#[test]
fn ports_card_sits_under_processes_not_full_width() {
    let (w, h) = (100u16, 45u16);
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
fn three_cards_render_side_by_side_at_full_width() {
    let c = draw_at(120, 40);
    for title in ["localhost", "claude", "others"] {
        assert!(c.contains(title), "card title {title:?} missing at 120x40");
    }
}

#[test]
fn narrow_terminals_get_the_single_grouped_card() {
    let c = draw_at(80, 24);
    assert!(c.contains("Ports"), "the grouped Ports card should render below 120 cols");
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

/// Measure the actual height of the card band at different terminal sizes.
/// Returns (card_band_total_height, content_rows) where content_rows = total - 2 borders.
fn card_band_dimensions(w: u16, h: u16) -> (usize, usize) {
    let lines = draw_grid(w, h);

    // Find the row where the card band starts (title row of localhost)
    let start_row = match lines.iter().position(|l| l.contains("localhost")) {
        Some(r) => r,
        None => return (0, 0),
    };

    // Find the row where the card band ends (last border line before next section or terminal end)
    let mut end_row = start_row;
    for (i, line) in lines.iter().enumerate().skip(start_row + 1) {
        // Stop when we hit a line that looks like the bottom border of the cards
        if line.contains("╯") || line.contains("┘") || i == lines.len() - 1 {
            end_row = i;
            break;
        }
    }

    let total_height = end_row - start_row + 1;
    let content_rows = total_height.saturating_sub(2); // subtract top and bottom borders
    (total_height, content_rows)
}

#[test]
fn card_band_has_adequate_space_at_120x30() {
    // At 120x30 with full tier (header 9 + gauges 12), body gets 9 rows.
    // The card band should get enough room to show 3 content rows.
    let (total, content) = card_band_dimensions(120, 30);
    eprintln!("120x30: card band total={} rows, content={} rows", total, content);
    assert_eq!(total, 5, "At 120x30 card band should be 5 rows total");
    assert_eq!(content, 3, "At 120x30 card band should have 3 content rows");
}

#[test]
fn card_band_reaches_full_height_at_160x45() {
    // At larger sizes, the card band should get its full 4 content rows.
    let (total, content) = card_band_dimensions(160, 45);
    eprintln!("160x45: card band total={} rows, content={} rows", total, content);
    assert_eq!(total, 6, "At 160x45 card band should be 6 rows total");
    assert_eq!(content, 4, "At 160x45 card band should have 4 content rows");
}

#[test]
fn card_band_at_120x40() {
    // At 120x40, measure the card band height for comprehensive coverage.
    let (total, content) = card_band_dimensions(120, 40);
    eprintln!("120x40: card band total={} rows, content={} rows", total, content);
    assert_eq!(total, 6, "At 120x40 card band should be 6 rows total");
    assert_eq!(content, 4, "At 120x40 card band should have 4 content rows");
}

#[test]
fn tier_boundary_is_exactly_120x30() {
    assert!(has_braille(&draw_at(120, 30)), "120x30 must be full tier");
    assert!(!has_braille(&draw_at(119, 30)), "119x30 must be compact");
    assert!(!has_braille(&draw_at(120, 29)), "120x29 must be compact");
}
