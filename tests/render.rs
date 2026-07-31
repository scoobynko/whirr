use ratatui::backend::TestBackend;
use ratatui::Terminal;
use whirr::app::{App, Focus};
use whirr::ui;
use whirr::ui::theme;

fn draw_at(w: u16, h: u16) -> String {
    let app = App::demo();
    draw_app_at(&app, w, h)
}

fn draw_app_at(app: &App, w: u16, h: u16) -> String {
    draw_buffer_at(app, w, h).content().iter().map(|c| c.symbol()).collect()
}

fn draw_buffer_at(app: &App, w: u16, h: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// `App::demo()` with `focus` overridden, for exercising the global footer's
/// per-focus content.
fn demo_with_focus(focus: Focus) -> App {
    let mut app = App::demo();
    app.focus = focus;
    app
}

fn has_braille(s: &str) -> bool {
    s.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
}

/// Row/col grid of a render, for asserting on exact panel boundaries rather
/// than "some text appears somewhere in the flattened buffer".
fn draw_grid(w: u16, h: u16) -> Vec<String> {
    draw_grid_app(&App::demo(), w, h)
}

fn draw_grid_app(app: &App, w: u16, h: u16) -> Vec<String> {
    let flat = draw_app_at(app, w, h);
    let cells: Vec<char> = flat.chars().collect();
    cells.chunks(w as usize).map(|row| row.iter().collect()).collect()
}

/// Row index of the first line containing `needle` — for a panel_block,
/// that's the row its title (and top border) is drawn on.
fn row_of(lines: &[String], needle: &str) -> usize {
    lines.iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("{needle:?} not found"))
}

/// The frame paints `theme::BASE` across itself before anything else
/// renders, so a widget that only sets a foreground (borders, titles, plain
/// text — the vast majority of the UI) must still composite on a `BASE`
/// background rather than the terminal's default. Sampled at a genuinely
/// empty patch of screen, on a card's border, and on plain body text inside
/// a card, at a size large enough for every panel to be present.
#[test]
fn frame_background_is_base_at_sampled_positions() {
    let app = App::demo();
    let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();

    // Top-left corner: outside every panel, genuinely empty screen space.
    assert_eq!(buf[(0, 0)].bg, theme::BASE, "empty top-left corner should carry the base background");

    let lines: Vec<String> = (0..buf.area.height)
        .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
        .collect();

    // A card's border cell: panel_block's border_style sets only `fg`, so
    // the border row's leftmost drawn character (its corner) must still
    // carry the base background underneath.
    let cpu_row = lines.iter().position(|l| l.contains("CPU")).expect("CPU card present");
    let border_x = lines[cpu_row].find(|c: char| c != ' ').expect("border corner present on CPU's title row");
    assert_eq!(
        buf[(border_x as u16, cpu_row as u16)].bg, theme::BASE,
        "CPU card's border cell should keep the base background under its fg-only border style"
    );

    // Plain body text inside a card (a non-selected process row): the
    // process table only sets `bg` on its selected row, so a different row's
    // fg-only text must still keep the base background.
    let processes_row = lines.iter().position(|l| l.contains("Processes")).expect("Processes card present");
    let body_y = (processes_row + 2) as u16; // row 0 (title+1) is index 0, the default-selected row
    assert_eq!(
        buf[(2, body_y)].bg, theme::BASE,
        "plain process row text should keep the base background, not Reset"
    );
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
// true and a compact-tier header (3) + gauges (10), one row is now taken off
// the bottom of the whole screen for the global keybind footer before this
// split ever runs, so the body gets 10 rows (not 11). Paired against Min(4),
// Max(13) hands the process table up to its cap but not below the point
// where ports would starve — at body height 10 that solves to (6, 4): the
// process table shrinks to 6 rows and ports gets exactly its Min(4) floor.
// This also pins that the header/gauges rows (resolved by the outer
// `Layout::split` before this body split ever runs) stay their fixed sizes
// regardless of what the body split below does.
#[test]
fn stock_80x24_fits_without_starving_header_or_ports() {
    let lines = draw_grid(80, 24);
    assert_eq!(lines.len(), 24);

    // Header is 5 rows, gauges 8: the first gauge card's title lands exactly
    // on row 5, unaffected by anything the body split below decides. (These
    // were 3 and 10 before the compact tier adopted the 5-row bitmap
    // wordmark; the two rows the header gained came out of the gauge band's
    // slack, not the body, so every body assertion below is unchanged.)
    assert_eq!(row_of(&lines, "CPU"), 5, "gauges row wasn't given its fixed height");

    // Body still starts at row 13 (5 + 8) and is still 10 rows. The process
    // table's Max(13) cap can't be met at that height, so it shrinks to 6
    // rows; Ports starts right after and gets its Min(4) floor.
    let processes_row = row_of(&lines, "Processes");
    assert_eq!(processes_row, 13, "process table didn't start where the body begins");
    let ports_row = row_of(&lines, "Ports");
    assert_eq!(ports_row - processes_row, 6, "process table should have shrunk to 6 rows (was 7 before the footer row was taken off the bottom), not its full 13-row cap");
    assert_eq!(ports_row, 19, "ports card should start right after the shrunk process table");
    // Ports keeps its Min(4) floor: 4 rows (19..=22). The last row (23) is
    // now the global footer, not part of the ports card — before the footer
    // existed, ports' floor happened to land exactly on the terminal's last
    // row; now it ends one row short of that to make room for the footer.
    assert_eq!(lines.len() - 2, ports_row + 3, "ports card should occupy exactly its Min(4) floor, one row above the global footer");
    assert!(lines[23].starts_with('↑'), "last row should be the global footer");
}

// At >=120 columns, `three_cards` is always active (it only checks width),
// so the old single-Ports-card body split (Max(13)/Min(4)) can no longer
// coexist with the full-tier header/gauges (which also require width>=120)
// — that scenario the pre-three-cards suite exercised here no longer
// exists. What replaces it: the three-card row is `Max(10)` (up to 8 content
// rows) when the body has room, but shrinks below that at tight heights
// (120x30 is floor-bound by Processes' Min(4) and gets only 3 — see the
// three-card layout tests below). At a generous body height all four of
// `demo()`'s sessions should fit, proving the row actually reached its full
// 8 content rows rather than staying shrunk to the 120x30 case's 3.
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

/// Hero digits and the header wordmark used to be drawn with foreground
/// quadrant/block characters, which seam in Terminal.app (see
/// `ui/font.rs`'s doc comment for why). They're background-filled cells now,
/// so this proves the fix by sampling `bg` colours off the buffer rather
/// than grepping rendered text for glyph characters — every hero/logo cell's
/// symbol is a plain space.
#[test]
fn full_tier_shows_hero_font_and_burst_fan() {
    let app = App::demo();
    let buf = draw_buffer_at(&app, 160, 45);
    assert!(has_braille(&draw_app_at(&app, 160, 45)), "burst fan missing");

    // Header wordmark: some cell must carry the accent colour as background,
    // and the count of such cells (in the header band, rows 0..9) must match
    // the "WHIRR" bitmap exactly.
    let wordmark_filled = (0..buf.area.width)
        .flat_map(|x| (0..9u16).map(move |y| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].style().bg == Some(theme::ACCENT))
        .count();
    let wordmark_expected: usize =
        whirr::ui::font::big_text("WHIRR").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
    assert_eq!(wordmark_filled, wordmark_expected, "header wordmark bitmap pixel count mismatch");

    // CPU hero (total_cpu 41 → "41%") and Power hero (6.4+1.2+0.3=7.9 →
    // "7.9 W") both use ACCENT; both are visible at 160x45. Nothing else
    // below the header paints ACCENT as a bg (Temp is AMBER at 88°C, Memory
    // is GREEN at Normal pressure), so the total accent-bg pixel count below
    // the header must equal exactly the sum of both hero bitmaps.
    let cpu_hero_expected: usize =
        whirr::ui::font::big_text("41%").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
    let power_hero_expected: usize =
        whirr::ui::font::big_text("7.9 W").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
    let accent_filled_below_header = (0..buf.area.width)
        .flat_map(|x| (9..buf.area.height).map(move |y| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].style().bg == Some(theme::ACCENT))
        .count();
    assert_eq!(
        accent_filled_below_header,
        cpu_hero_expected + power_hero_expected,
        "cpu+power hero bitmap pixel count mismatch for \"41%\" and \"7.9 W\""
    );
}

#[test]
fn compact_tier_shares_the_brand_assets_but_not_the_hero_numbers() {
    // The compact tier used to be the whole pre-refresh design: its own
    // block-glyph wordmark, four hand-drawn `✻` fan frames, and a thermometer.
    // It now shares the burst and the bitmap wordmark with the larger tiers —
    // what still separates it is that the gauge cards carry plain readouts
    // rather than hero numbers.
    let app = App::demo();
    let c = draw_app_at(&app, 80, 24);
    assert!(has_braille(&c), "burst fan should render at 80x24");
    assert!(!c.contains('✻'), "hand-drawn fan should be gone");
    assert!(!c.contains('▐'), "temp thermometer should be gone");

    // The only ACCENT-backed bitmap at this size is the header wordmark: the
    // gauge cards are below the hero threshold, so none of them paint one.
    let buf = draw_buffer_at(&app, 80, 24);
    let accent_cells = buf.content().iter().filter(|c| c.style().bg == Some(theme::ACCENT)).count();
    let wordmark: usize =
        whirr::ui::font::big_text("WHIRR").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
    assert_eq!(accent_cells, wordmark, "compact tier should paint the wordmark bitmap and no hero numbers");
    assert!(c.contains("88.0°C"), "compact temp readout missing");
}

/// Measure the actual height of both the processes and card bands at different terminal sizes.
/// Returns (processes_band_height, card_band_total_height, card_band_content_rows).
fn band_dimensions(w: u16, h: u16) -> (usize, usize, usize) {
    let lines = draw_grid(w, h);

    // Find the row where the processes band starts (title row of Processes)
    let processes_start = match lines.iter().position(|l| l.contains("Processes")) {
        Some(r) => r,
        None => return (0, 0, 0),
    };

    // Find the row where the card band starts (title row of localhost)
    let card_start = match lines.iter().position(|l| l.contains("localhost")) {
        Some(r) => r,
        None => return (0, 0, 0),
    };

    let processes_band_height = card_start - processes_start;

    // Find the row where the card band ends (last border line before next section or terminal end)
    let mut card_end = card_start;
    for (i, line) in lines.iter().enumerate().skip(card_start + 1) {
        // Stop when we hit a line that looks like the bottom border of the cards
        if line.contains("╯") || line.contains("┘") || i == lines.len() - 1 {
            card_end = i;
            break;
        }
    }

    let card_total_height = card_end - card_start + 1;
    let card_content_rows = card_total_height.saturating_sub(2); // subtract top and bottom borders
    (processes_band_height, card_total_height, card_content_rows)
}

/// Measure the actual height of the card band at different terminal sizes.
/// Returns (card_band_total_height, content_rows) where content_rows = total - 2 borders.
fn card_band_dimensions(w: u16, h: u16) -> (usize, usize) {
    let (_, total, content) = band_dimensions(w, h);
    (total, content)
}

#[test]
fn card_band_has_adequate_space_at_120x30() {
    // At 120x30 with full tier (header 9 + gauges 12) and one row now taken
    // off the bottom of the whole screen for the global keybind footer,
    // body gets only 8 rows (was 9 before the footer existed), so the split
    // is floor-bound: Processes claims its Min(3) floor (1 header + 1 footer + 1 content)
    // and the card band gets whatever's left (5 rows = 2 borders + 3 content).
    // This recovers the 3-content-row target the user had asked for the card band.
    let (total, content) = card_band_dimensions(120, 30);
    eprintln!("120x30: card band total={} rows, content={} rows", total, content);
    assert_eq!(total, 5, "At 120x30 card band should be 5 rows total");
    assert_eq!(content, 3, "At 120x30 card band should have 3 content rows");
}

#[test]
fn card_band_reaches_full_height_at_160x45() {
    // At larger sizes, the card band should reach its full Max(10) cap: 8 content rows.
    let (total, content) = card_band_dimensions(160, 45);
    eprintln!("160x45: card band total={} rows, content={} rows", total, content);
    assert_eq!(total, 10, "At 160x45 card band should be 10 rows total");
    assert_eq!(content, 8, "At 160x45 card band should have 8 content rows");
}

#[test]
fn card_band_at_120x40() {
    // At 120x40, measure the card band height for comprehensive coverage.
    let (total, content) = card_band_dimensions(120, 40);
    eprintln!("120x40: card band total={} rows, content={} rows", total, content);
    assert_eq!(total, 10, "At 120x40 card band should be 10 rows total");
    assert_eq!(content, 8, "At 120x40 card band should have 8 content rows");
}

#[test]
fn tier_boundary_is_exactly_120x30() {
    // The burst fan renders at every tier now, so it can no longer tell them
    // apart. The header band's height can: 9 rows for full/grid, 5 for
    // compact — so the first gauge card's title row is the discriminator.
    // 119x30 and 120x29 are both too small for the grid tier as well (it
    // needs 40 rows), so both must land on compact.
    let first_gauge_row = |w, h| row_of(&draw_grid(w, h), "CPU");
    assert_eq!(first_gauge_row(120, 30), 9, "120x30 must be full tier");
    assert_eq!(first_gauge_row(119, 30), 5, "119x30 must be compact");
    assert_eq!(first_gauge_row(120, 29), 5, "120x29 must be compact");
}

// --- global footer -------------------------------------------------------

#[test]
fn footer_kill_shows_only_for_killable_panels() {
    for focus in [Focus::Processes, Focus::Localhost] {
        let c = draw_app_at(&demo_with_focus(focus), 120, 40);
        assert!(c.contains("k kill"), "{focus:?} should show k kill");
    }
    for focus in [Focus::Sessions, Focus::Others] {
        let c = draw_app_at(&demo_with_focus(focus), 120, 40);
        assert!(!c.contains("k kill"), "{focus:?} must not show k kill — k is inert there");
    }
}

#[test]
fn footer_sort_shows_only_for_processes() {
    let c = draw_app_at(&demo_with_focus(Focus::Processes), 120, 40);
    assert!(c.contains("c/m sort"), "Processes should show c/m sort");
    for focus in [Focus::Localhost, Focus::Sessions, Focus::Others] {
        let c = draw_app_at(&demo_with_focus(focus), 120, 40);
        assert!(!c.contains("c/m sort"), "{focus:?} must not show c/m sort — sorting is a process-table concept");
    }
}

#[test]
fn footer_quit_and_tab_focus_show_for_every_focus() {
    for focus in [Focus::Processes, Focus::Localhost, Focus::Sessions, Focus::Others] {
        let c = draw_app_at(&demo_with_focus(focus), 120, 40);
        assert!(c.contains("tab focus"), "{focus:?} should show tab focus (global)");
        assert!(c.contains("q quit"), "{focus:?} should show q quit (global)");
        assert!(c.contains("↑↓ select"), "{focus:?} should show ↑↓ select (global)");
    }
}

#[test]
fn footer_renders_on_the_last_row_of_the_screen() {
    for (w, h) in [(120u16, 30u16), (160, 45), (80, 24)] {
        let lines = draw_grid(w, h);
        assert_eq!(lines.len(), h as usize);
        let last = &lines[h as usize - 1];
        assert!(
            last.contains("↑↓ select") && last.contains("tab focus") && last.contains("q quit"),
            "{w}x{h}: footer should render on the screen's last row, got {last:?}"
        );
    }
}

#[test]
fn processes_card_no_longer_contains_the_old_hint_text() {
    // Focus::Sessions so the global footer itself can never happen to
    // produce this exact 5-item string (it drops c/m sort and k kill for
    // Sessions) — an absence check with Focus::Processes would be
    // meaningless, since the footer legitimately renders that same string
    // for that focus. With the footer ruled out, this can only fail if the
    // Processes card still renders its own copy of the old hint line.
    let c = draw_app_at(&demo_with_focus(Focus::Sessions), 160, 45);
    assert!(
        !c.contains("↑↓ select · c/m sort · k kill · tab focus · q quit"),
        "the old combined hint line should no longer live inside the Processes card"
    );
}

/// The selected process row's highlight must be one unbroken block. Every
/// span in the row derives its style from `base` — which carries
/// `bg(BG_CELL)` when selected — except the two micro-bars, which were
/// built from a fresh `Style::default().fg(gradient(..))`. That dropped the
/// background, so the bar cells fell through to the frame's `BASE` and cut
/// two dark notches out of the highlight. Asserts contiguity across the
/// row's whole drawn extent rather than just checking the bar cells, so any
/// future span that forgets the row background also trips this.
#[test]
fn selected_process_row_highlight_is_contiguous_across_its_bars() {
    let app = App::demo(); // focus defaults to Processes, selected to 0
    let buf = draw_buffer_at(&app, 160, 45);

    let lines: Vec<String> = (0..buf.area.height)
        .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
        .collect();
    let y = lines
        .iter()
        .position(|l| l.contains("kernel_task"))
        .expect("demo's top process row should be on screen") as u16;

    let highlighted: Vec<u16> = (0..buf.area.width)
        .filter(|&x| buf[(x, y)].style().bg == Some(theme::BG_CELL))
        .collect();
    assert!(!highlighted.is_empty(), "selected row should be highlighted at all");

    let (first, last) = (highlighted[0], *highlighted.last().unwrap());
    let gaps: Vec<(u16, String, Option<ratatui::style::Color>)> = (first..=last)
        .filter(|&x| buf[(x, y)].style().bg != Some(theme::BG_CELL))
        .map(|x| (x, buf[(x, y)].symbol().to_string(), buf[(x, y)].style().bg))
        .collect();
    assert!(
        gaps.is_empty(),
        "selected row {y} highlight breaks at {gaps:?} (row spans x={first}..={last})"
    );

    // Contiguity alone can't see the mem bar: it's the row's *last* span, so
    // an unhighlighted mem bar shrinks `last` instead of registering as an
    // interior gap. Check the bar cells by symbol as well, so both bars are
    // covered.
    let unhighlighted_bars: Vec<u16> = (0..buf.area.width)
        .filter(|&x| matches!(buf[(x, y)].symbol(), "▮" | "▯"))
        .filter(|&x| buf[(x, y)].style().bg != Some(theme::BG_CELL))
        .collect();
    assert!(
        unhighlighted_bars.is_empty(),
        "selected row {y}: micro-bar cells at x={unhighlighted_bars:?} dropped the row background"
    );
}

/// Moving the cursor in one card must not scroll a different, unfocused card.
/// `App` used to keep a single shared cursor and `processes::render` computed
/// its offset from it without checking focus, so scrolling the sessions card at
/// 120x30 — where the process table is squeezed to one content row — pushed the
/// top process off screen.
#[test]
fn scrolling_one_card_does_not_scroll_an_unfocused_one() {
    let mut app = demo_with_focus(Focus::Sessions);
    assert!(draw_app_at(&app, 120, 30).contains("kernel_task"), "top process visible to begin with");
    for _ in 0..3 {
        app.on_key(ratatui::crossterm::event::KeyEvent::from(
            ratatui::crossterm::event::KeyCode::Down,
        ));
    }
    assert_eq!(app.selected(), 3, "the sessions cursor should have moved");
    assert!(
        draw_app_at(&app, 120, 30).contains("kernel_task"),
        "the unfocused process table scrolled because another card's cursor moved"
    );
}

/// A tick where lsof found no listening sockets must not blank the sessions
/// card: sessions come from a pid walk that never consults lsof. They used to
/// share one match arm in `sampler::slow`, so an empty (or failed) port scan
/// took the sessions down with it.
#[test]
fn an_empty_port_scan_leaves_the_sessions_card_intact() {
    use whirr::sampler::{SlowSnap, Snapshot};
    let mut app = App::demo();
    let sessions = app.sessions().to_vec();
    assert!(!sessions.is_empty(), "demo has sessions to lose");
    // What `slow::run` now sends when lsof matches nothing: no rows, but the
    // independently-scanned sessions still present.
    app.ingest(Snapshot::Slow(SlowSnap { rows: Vec::new(), sessions, stale: false }));

    let out = draw_app_at(&app, 160, 45);
    assert!(out.contains("ttys020"), "claude sessions vanished with the ports");
    assert!(out.contains("no listening ports"), "the ports cards should be the empty ones");
}

/// Narrow-but-tall terminals get the full visual design via a 2x2 gauge grid
/// rather than dropping to the compact tier. 103x45 fails the `full` gate on
/// width (needs 120 for four hero cards abreast) but has ample height, so the
/// gauges stack two-by-two and every card reaches hero width.
#[test]
fn narrow_but_tall_gets_the_hero_design_in_a_two_by_two_grid() {
    let app = App::demo();
    let lines = draw_grid_app(&app, 103, 45);

    // Header keeps its full 9-row band, so the first gauge row starts at 9.
    assert!(has_braille(&draw_app_at(&app, 103, 45)), "burst fan missing at 103x45");
    assert_eq!(row_of(&lines, "CPU"), 9, "first gauge band should start below the 9-row header");
    // Two cards per band: CPU/Temp share a title row, Power/Memory share the next.
    assert_eq!(row_of(&lines, "Temp"), 9, "Temp should sit beside CPU, not below it");
    assert_eq!(row_of(&lines, "Power"), 21, "Power should start the second gauge band");
    assert_eq!(row_of(&lines, "Memory"), 21, "Memory should sit beside Power");

    // Hero bitmaps actually render — the whole point of the grid. CPU's "41%"
    // hero is ACCENT-backed, as is the header wordmark.
    let buf = draw_buffer_at(&app, 103, 45);
    let hero_cells = (0..buf.area.width)
        .flat_map(|x| (9..21u16).map(move |y| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].style().bg == Some(theme::ACCENT))
        .count();
    let expected: usize =
        whirr::ui::font::big_text("41%").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
    assert_eq!(hero_cells, expected, "CPU hero bitmap missing or wrong size in the 2x2 grid");
}

/// Both floors of the 2x2 grid, checked one step below each. The grid costs a
/// second 12-row band, so it needs 40 rows (header 9 + bands 24 + body 6 +
/// footer 1); and it needs 70 columns so all four cards are present. Falling
/// below either must land back on the compact tier, not on a broken layout.
#[test]
fn two_by_two_grid_falls_back_below_either_floor() {
    for (w, h, why) in [(103u16, 39u16, "one row below the 40-row floor"), (69, 45, "one column below the 70-col floor")] {
        let app = App::demo();
        let lines = draw_grid_app(&app, w, h);
        assert_eq!(row_of(&lines, "CPU"), 5, "{w}x{h} ({why}): compact header is 5 rows, so gauges start at row 5");
    }
}
