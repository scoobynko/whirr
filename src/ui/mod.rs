pub mod font;
pub mod burst;
pub mod cpu;
pub mod header;
pub mod memory;
pub mod network;
pub mod ports;
pub mod power;
pub mod processes;
pub mod sessions;
pub mod spark;
pub mod temp;
pub mod theme;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use crate::app::{App, Focus, MAX_VISIBLE_PROCS};

/// Final responsive layout. Panels drop out in priority order as space gets
/// tight: ports first, then network, then power, then temp — processes and
/// CPU always survive. The gauges row splits into as many equal columns as
/// there are visible gauge panels (cpu + temp? + power? + memory) — except in
/// the grid tier described below, which is always exactly four cards arranged
/// in two rows of two.
///
/// The body below the gauges has two shapes:
///
/// - **Width >= 120** (`three_cards`): the body splits into two rows —
///   processes (with network alongside it, if it fits) on top, and three
///   equal-width cards (localhost, claude sessions, others) in a variable-height
///   row below. The card row uses constraints `Min(3)` (processes) and `Max(10)`
///   (cards): at 120×30 the body is 8 rows (after footer takes 1), so processes
///   gets 3 rows (1 header + 1 footer + 1 content) and cards takes 5 rows
///   (2 borders + 3 content), giving each card 3 visible rows. At >=120×40 the
///   body has room for both processes to grow past its floor and cards to reach
///   their 10-row max (2 borders + 8 content). Splitting the three port/session
///   groups into their own cards is only worth the width when each can show a
///   useful number of rows; below 120 columns they would be too cramped to read.
/// - **Narrower**: the single-card `render_left_column` shape below, with the
///   grouped `ports::render` card (all three groups, headers, in one card)
///   standing in for the three-card row.
///
/// The body below the gauges (when narrower than 120 cols) splits into a left
/// column (processes stacked over ports) and a right column (network at full
/// body height). Within the left column the process table is capped at
/// `MAX_VISIBLE_PROCS` rows (+ footer + borders) via `Max`, paired with a
/// `Min(4)` floor for the ports card below it: the process table claims up to
/// its cap and ports gets whatever's left, but never below its floor — so at
/// tight body heights the process table shrinks (fewer visible rows) while
/// ports keeps its minimum, and once there's slack beyond both the process
/// table takes its full cap and ports grows into the rest. (`Length(13)` paired
/// with the same `Min(4)` produces an identical split in ratatui 0.29 at every
/// body height tested — `Max` is used here because it documents the "cap, not
/// a fixed size" intent, not because it behaves differently from `Length` in
/// this pairing.) Either way, this constraint can only ever move space between
/// the process table and ports: the header and gauges rows are resolved by
/// the outer `Layout::split` above, before `render_left_column` is ever
/// called, so nothing inside it can squeeze them.
///
/// Three visual tiers, in order of preference:
///
/// - **Full (>=120x30)**: padded header with the braille burst fan (9 rows)
///   and one row of four hero-number gauge cards (12 rows).
/// - **Grid (>=70x40, narrower than full)**: the same header and the same
///   hero cards, but the four gauges stacked 2x2 across two 12-row bands.
///   A hero card needs 30 columns, so four abreast need 120 while two need
///   only 60 — a terminal that is merely narrow can still show the full
///   design by trading a second band for the width. This is what keeps
///   sizes like 103x45 on the real visuals.
/// - **Compact (anything smaller)**: the pre-refresh design — 3-row header
///   with the hand-drawn fan, 10-row gauges, no hero numbers.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    // Paint the whole frame with the near-black base first, before anything
    // else renders, so every widget composites on top of it. A borderless
    // Block fills its full area's background via Buffer::set_style, which
    // only patches the bg channel — later fg-only styles (most widgets here)
    // leave this bg untouched.
    f.render_widget(Block::default().style(Style::default().bg(theme::BASE)), area);
    let show_ports = area.height >= 20;
    let show_network = area.height >= 16;
    let show_power = area.width >= 70;
    let show_temp = area.width >= 50;
    // Full visual tier: padded header with the braille burst fan, hero-number
    // gauge cards. Needs width for the ~27-col hero strings (4 cards x 30
    // cols) and height for header 9 + gauges 12 + a useful body.
    let full = area.height >= 30 && area.width >= 120;
    // Narrow-but-tall terminals used to fail the `full` gate on width alone
    // and drop the entire frame to the compact design. But the width failure
    // is an artifact of the 1x4 gauge row, not a real constraint: a hero card
    // needs 30 columns (28 inner + 2 borders), so four abreast need 120 while
    // two abreast need only 60. Stacking the gauges 2x2 gets every card to
    // hero width on terminals that are merely narrow, at the cost of a second
    // 12-row band — hence the 40-row floor (header 9 + two bands 24 + body 6 +
    // footer 1). The 70-column floor is `show_power`'s, so the grid always has
    // exactly four cards to place and never a half-empty second row.
    let grid = !full && area.width >= 70 && area.height >= 40;
    // Both arrangements get the padded header and hero-sized cards; only the
    // compact tier keeps the old 3-row header and 10-row gauges.
    let hero_tier = full || grid;

    // One bare row for the global keybind footer, taken off the bottom
    // before anything else is laid out — it costs exactly one row and has
    // no border/block, so it doesn't shift where the header/gauges land.
    let screen = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
    let content = screen[0];
    render_footer(f, screen[1], app);

    let chunks = Layout::vertical([
        Constraint::Length(if hero_tier { 9 } else { 3 }),
        Constraint::Length(match (full, grid) {
            (_, true) => 24, // two 12-row bands
            (true, _) => 12,
            _ => 10,
        }),
        Constraint::Min(6),
    ])
    .split(content);

    header::render(f, chunks[0], app);

    if grid {
        // Two bands of two. `grid` already guarantees show_temp and show_power,
        // so all four cards are always present and no cell goes empty.
        let half = |r: Rect| Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(r);
        let bands =
            Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(chunks[1]);
        let (top, bottom) = (half(bands[0]), half(bands[1]));
        cpu::render(f, top[0], app);
        temp::render(f, top[1], app);
        power::render(f, bottom[0], app);
        memory::render(f, bottom[1], app);
    } else {
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
    }

    let body = chunks[2];
    // Three side-by-side cards need ~40 columns each; below that the single
    // grouped Ports card carries all three groups instead.
    let three_cards = area.width >= 120;
    if three_cards {
        // Row A holds processes and network; row B the three cards. Row B gets
        // up to 10 (2 borders + 8 content rows), with Processes getting a floor of 3.
        // At 120x30 the floor allows processes a single row, prioritizing card height.
        let rows = Layout::vertical([Constraint::Min(3), Constraint::Max(10)]).split(body);
        if show_network {
            let cols = Layout::horizontal([Constraint::Ratio(3, 5), Constraint::Ratio(2, 5)])
                .split(rows[0]);
            processes::render(f, cols[0], app);
            network::render(f, cols[1], app);
        } else {
            processes::render(f, rows[0], app);
        }
        let cards = Layout::horizontal([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(rows[1]);
        ports::render_localhost(f, cards[0], app);
        sessions::render(f, cards[1], app);
        ports::render_others(f, cards[2], app);
    } else if show_network {
        let cols = Layout::horizontal([Constraint::Ratio(3, 5), Constraint::Ratio(2, 5)])
            .split(body);
        render_left_column(f, cols[0], app, show_ports);
        network::render(f, cols[1], app);
    } else {
        render_left_column(f, body, app, show_ports);
    }
}

/// Global keybind footer: one bare row, left-aligned, no border. Only shows
/// keys that actually do something for the current focus — `c/m sort` is a
/// process-table concept, `k kill` only applies to the two killable panels
/// (Processes, Localhost) — everything else is always available. Items sit
/// in fixed positions (select, sort?, kill?, tab, quit) so the line reads as
/// entries appearing/disappearing as focus changes, not reshuffling.
fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let show_sort = matches!(app.focus, Focus::Processes);
    let show_kill = matches!(app.focus, Focus::Processes | Focus::Localhost);
    let items: [Option<&str>; 5] = [
        Some("↑↓ select"),
        show_sort.then_some("c/m sort"),
        show_kill.then_some("k kill"),
        Some("tab focus"),
        Some("q quit"),
    ];
    let text = items.into_iter().flatten().collect::<Vec<_>>().join(" · ");
    f.render_widget(Paragraph::new(text).style(Style::default().fg(theme::DIM)), area);
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
