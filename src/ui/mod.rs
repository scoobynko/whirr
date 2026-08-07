pub mod burst;
pub mod cpu;
pub mod font;
pub mod gauge;
pub mod header;
pub mod memory;
pub mod modal;
pub mod network;
pub mod ports;
pub mod power;
pub mod processes;
pub mod screen;
pub mod scroll;
pub mod sessions;
pub mod spark;
pub mod temp;
pub mod text;
pub mod theme;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use crate::app::{App, Focus};
use screen::{Body, Gauge, Screen, Tier};

/// Draw one frame.
///
/// All of the "does this fit" reasoning lives in `screen::Screen::resolve`,
/// which turns the terminal size into a value; this function only places what
/// that value says to place. The vertical split is the same at every tier —
/// header, gauges, body, and a bare footer row taken off the bottom first (it
/// has no block, so it can't shift where the header and gauges land).
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    // Paint the whole frame with the near-black base first, before anything
    // else renders, so every widget composites on top of it. A borderless
    // Block fills its full area's background via Buffer::set_style, which
    // only patches the bg channel — later fg-only styles (most widgets here)
    // leave this bg untouched.
    f.render_widget(Block::default().style(Style::default().bg(theme::BASE)), area);

    let screen = Screen::resolve(area);
    let split = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
    render_footer(f, split[1], app);

    let chunks = Layout::vertical([
        Constraint::Length(screen.tier.header_rows()),
        Constraint::Length(screen.tier.gauge_rows()),
        Constraint::Min(6),
    ])
    .split(split[0]);

    header::render(f, chunks[0], app);
    render_gauges(f, chunks[1], app, &screen);
    render_body(f, chunks[2], app, &screen);

    // Last, over everything, and over the *whole* frame rather than the panel
    // the target came from — a confirmation raised on the localhost card used
    // to appear inside the Processes box two panels away.
    if let Some((pid, name)) = &app.pending_kill {
        render_kill_dialog(f, area, *pid, name);
    } else if let Some(pick) = &app.pending_port_pick {
        render_port_picker(f, area, pick);
    }
}

/// "Which of this row's ports did you mean?" — asked only when the row offers
/// more than one, because `o` guessing the lowest opened Storybook's Vite port
/// instead of Storybook.
///
/// Accented rather than red: nothing here is destructive.
fn render_port_picker(f: &mut Frame, area: Rect, pick: &crate::app::PortPick) {
    let mut lines = vec![
        Line::styled(pick.label.clone(), Style::default().fg(theme::TEXT).bold()),
        Line::from(""),
    ];
    for (i, port) in pick.ports.iter().enumerate() {
        lines.push(Line::from(vec![
            // The digit that opens it, next to what it opens.
            Span::styled(format!("{}", i + 1), Style::default().fg(theme::ACCENT).bold()),
            Span::styled(format!("  :{port}"), Style::default().fg(theme::TEXT)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("n", Style::default().fg(theme::TEXT).bold()),
        Span::styled(" cancel", Style::default().fg(theme::DIM)),
    ]));
    modal::render(f, area, "open which port?", lines, theme::ACCENT);
}

/// "You are about to send SIGTERM to this" — the one irreversible thing whirr
/// does, so it gets the red accent and spells out both what dies and how to
/// back out.
fn render_kill_dialog(f: &mut Frame, area: Rect, pid: i32, name: &str) {
    let lines = vec![
        Line::styled(name.to_string(), Style::default().fg(theme::TEXT).bold()),
        Line::styled(format!("pid {pid}"), Style::default().fg(theme::DIM)),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().fg(theme::RED).bold()),
            Span::styled(" confirm", Style::default().fg(theme::TEXT)),
            Span::styled("   ", Style::default()),
            Span::styled("n", Style::default().fg(theme::TEXT).bold()),
            Span::styled(" cancel", Style::default().fg(theme::TEXT)),
        ]),
    ];
    modal::render(f, area, "confirm kill", lines, theme::RED);
}

/// The gauge band. `Tier::Grid` stacks the four cards 2x2 — `Screen` only
/// produces that tier when all four are present, so `zip` can never leave a
/// cell unpainted.
fn render_gauges(f: &mut Frame, area: Rect, app: &App, screen: &Screen) {
    let half = |r: Rect| {
        Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(r).to_vec()
    };
    let cells: Vec<Rect> = match screen.tier {
        Tier::Grid => {
            let bands =
                Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(area);
            [half(bands[0]), half(bands[1])].concat()
        }
        _ => {
            let n = screen.gauges.len() as u32;
            Layout::horizontal(vec![Constraint::Ratio(1, n); n as usize]).split(area).to_vec()
        }
    };
    for (gauge, cell) in screen.gauges.iter().zip(cells) {
        match gauge {
            Gauge::Cpu => cpu::render(f, cell, app),
            Gauge::Temp => temp::render(f, cell, app),
            Gauge::Power => power::render(f, cell, app),
            Gauge::Memory => memory::render(f, cell, app),
        }
    }
}

/// Everything below the gauge band. The network card takes a column beside the
/// left content in every shape that has it, so that split happens once here
/// rather than inside each arm.
fn render_body(f: &mut Frame, area: Rect, app: &App, screen: &Screen) {
    match screen.body {
        Body::ThreeCards => {
            // Row A holds processes (and network); row B the three cards. Row B
            // gets up to 10 rows (2 borders + 8 content), with processes floored
            // at 3 — at 120x30 that floor leaves processes a single content row,
            // prioritising card height.
            let rows = Layout::vertical([Constraint::Min(3), Constraint::Max(10)]).split(area);
            let procs = beside_network(f, rows[0], app, screen);
            processes::render(f, procs, app);
            let cards = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(rows[1]);
            ports::render(f, cards[0], app, ports::Card::Localhost);
            sessions::render(f, cards[1], app);
            ports::render(f, cards[2], app, ports::Card::Others);
        }
        Body::ProcessesOverPorts => {
            /// Process rows the table is *budgeted* in this body — the only
            /// place a number like this belongs. Here the table and the ports
            /// card share one column, so the table has to stop somewhere or
            /// ports starves; in `ThreeCards` the cards sit below and the
            /// table simply takes the rest of the height.
            ///
            /// This used to be `app::MAX_VISIBLE_PROCS` and also truncated the
            /// process list itself, which is what made a tall three-card panel
            /// draw ten rows over a stack of blank ones.
            const PROC_PANEL_ROWS: u16 = 10;
            let left = beside_network(f, area, app, screen);
            // The process table claims up to its cap (10 rows + footer +
            // borders) and ports gets the rest, but never below its floor: at
            // tight heights the table shrinks while ports keeps its minimum,
            // and past both the table takes its full cap and ports grows into
            // the slack. This can only ever move space between these two — the
            // header and gauge bands were resolved by `draw`'s split above.
            let rows = Layout::vertical([
                Constraint::Max(PROC_PANEL_ROWS + 3),
                Constraint::Min(4),
            ])
            .split(left);
            processes::render(f, rows[0], app);
            ports::render(f, rows[1], app, ports::Card::Combined);
        }
        Body::ProcessesOnly => {
            let procs = beside_network(f, area, app, screen);
            processes::render(f, procs, app);
        }
    }
}

/// Give the network card the right-hand ~2/5 of `area` when it fits, and
/// return what's left for the caller's own content.
fn beside_network(f: &mut Frame, area: Rect, app: &App, screen: &Screen) -> Rect {
    if !screen.network {
        return area;
    }
    let cols =
        Layout::horizontal([Constraint::Ratio(3, 5), Constraint::Ratio(2, 5)]).split(area);
    network::render(f, cols[1], app);
    cols[0]
}

/// Global keybind footer: one bare row, left-aligned, no border. Only shows
/// keys that actually do something for the current focus — `c/m sort` is a
/// process-table concept, `k kill` only applies to the two killable panels
/// (Processes, Localhost) — everything else is always available. Items sit
/// in fixed positions (select, sort?, kill?, tab, quit) so the line reads as
/// entries appearing/disappearing as focus changes, not reshuffling.
fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    // While a dialog is up every other key is inert, so advertising them
    // would be a lie. The footer becomes the dialog's own key legend.
    if app.pending_kill.is_some() {
        let text = Line::from(vec![
            Span::styled("y", Style::default().fg(theme::RED).bold()),
            Span::styled(" confirm · ", Style::default().fg(theme::DIM)),
            Span::styled("n", Style::default().fg(theme::TEXT).bold()),
            Span::styled(" cancel", Style::default().fg(theme::DIM)),
        ]);
        f.render_widget(Paragraph::new(text), area);
        return;
    }
    if let Some(pick) = &app.pending_port_pick {
        let text = Line::from(vec![
            Span::styled(
                format!("1-{}", pick.ports.len()),
                Style::default().fg(theme::ACCENT).bold(),
            ),
            Span::styled(" open · ", Style::default().fg(theme::DIM)),
            Span::styled("n", Style::default().fg(theme::TEXT).bold()),
            Span::styled(" cancel", Style::default().fg(theme::DIM)),
        ]);
        f.render_widget(Paragraph::new(text), area);
        return;
    }
    let show_sort = matches!(app.focus, Focus::Processes);
    let show_kill = matches!(app.focus, Focus::Processes | Focus::Localhost);
    let show_open = matches!(app.focus, Focus::Localhost);
    let items: [Option<&str>; 6] = [
        Some("↑↓ select"),
        show_sort.then_some("c/m sort"),
        show_open.then_some("o open"),
        show_kill.then_some("k kill"),
        Some("tab focus"),
        Some("q quit"),
    ];
    let text = items.into_iter().flatten().collect::<Vec<_>>().join(" · ");
    f.render_widget(Paragraph::new(text).style(Style::default().fg(theme::DIM)), area);
}
