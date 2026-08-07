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
    // Skipped when the user asked for the terminal's own background: whirr
    // painting edge to edge is what overrides a themed or translucent
    // terminal. `theme.base` still holds a real colour either way — it is the
    // anchor `ramp` and `blend` darken toward.
    if app.theme.paint_bg {
        f.render_widget(Block::default().style(Style::default().bg(app.theme.base)), area);
    }

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
    if app.settings_open {
        render_settings(f, area, app);
    } else if let Some((pid, name)) = &app.pending_kill {
        render_kill_dialog(f, area, &app.theme, *pid, name);
    } else if let Some(pick) = &app.pending_port_pick {
        render_port_picker(f, area, &app.theme, pick);
    }
}

/// The settings dialog: one row per choice, current value on the right.
///
/// Changes apply as you make them rather than on a confirm step — the whole
/// argument for a dialog over a config file is seeing the palette while you
/// are choosing it.
fn render_settings(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let rows: [(&str, String); App::SETTINGS_ROWS] = [
        ("theme", app.settings.palette.label().to_string()),
        ("accent", app.settings.accent.label().to_string()),
        (
            "background",
            // Under the light palette this row is inert, and it says so
            // rather than accepting a keypress that changes nothing.
            match (app.settings.terminal_bg_available(), app.settings.terminal_bg) {
                (false, _) => "painted (light)".to_string(),
                (true, true) => "terminal".to_string(),
                (true, false) => "painted".to_string(),
            },
        ),
        ("fan", if app.settings.fan { "on".into() } else { "off".into() }),
    ];
    // The widest label and value decide the column, so the values line up
    // instead of ragged-right.
    let label_w = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0);
    let value_w = rows.iter().map(|(_, v)| v.chars().count()).max().unwrap_or(0);

    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, (label, value))| {
            let selected = i == app.settings_row;
            // An inert row is dimmed even under the cursor: the cursor says
            // "here", the colour says "nothing to change".
            let locked = i == 2 && !app.settings.terminal_bg_available();
            let (marker, style) = if locked {
                ("  ", Style::default().fg(t.dim))
            } else if selected {
                ("› ", Style::default().fg(t.accent).bold())
            } else {
                ("  ", Style::default().fg(t.text))
            };
            Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{label:<label_w$}   "), style),
                Span::styled(
                    format!("{value:>value_w$}"),
                    if selected && !locked { style } else { Style::default().fg(t.dim) },
                ),
            ])
        })
        .collect();

    modal::render(f, area, t, "settings", lines, t.accent);
}

/// "Which of this row's ports did you mean?" — asked only when the row offers
/// more than one, because `o` guessing the lowest opened Storybook's Vite port
/// instead of Storybook.
///
/// Accented rather than red: nothing here is destructive.
fn render_port_picker(f: &mut Frame, area: Rect, theme: &theme::Theme, pick: &crate::app::PortPick) {
    let mut lines = vec![
        Line::styled(pick.label.clone(), Style::default().fg(theme.text).bold()),
        Line::from(""),
    ];
    for (i, port) in pick.ports.iter().enumerate() {
        lines.push(Line::from(vec![
            // The digit that opens it, next to what it opens.
            Span::styled(format!("{}", i + 1), Style::default().fg(theme.accent).bold()),
            Span::styled(format!("  :{port}"), Style::default().fg(theme.text)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("n", Style::default().fg(theme.text).bold()),
        Span::styled(" cancel", Style::default().fg(theme.dim)),
    ]));
    modal::render(f, area, theme, "open which port?", lines, theme.accent);
}

/// "You are about to send SIGTERM to this" — the one irreversible thing whirr
/// does, so it gets the red accent and spells out both what dies and how to
/// back out.
fn render_kill_dialog(f: &mut Frame, area: Rect, theme: &theme::Theme, pid: i32, name: &str) {
    let lines = vec![
        Line::styled(name.to_string(), Style::default().fg(theme.text).bold()),
        Line::styled(format!("pid {pid}"), Style::default().fg(theme.dim)),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().fg(theme.red).bold()),
            Span::styled(" confirm", Style::default().fg(theme.text)),
            Span::styled("   ", Style::default()),
            Span::styled("n", Style::default().fg(theme.text).bold()),
            Span::styled(" cancel", Style::default().fg(theme.text)),
        ]),
    ];
    modal::render(f, area, theme, "confirm kill", lines, theme.red);
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
            Span::styled("y", Style::default().fg(app.theme.red).bold()),
            Span::styled(" confirm · ", Style::default().fg(app.theme.dim)),
            Span::styled("n", Style::default().fg(app.theme.text).bold()),
            Span::styled(" cancel", Style::default().fg(app.theme.dim)),
        ]);
        f.render_widget(Paragraph::new(text), area);
        return;
    }
    if app.settings_open {
        let text = Line::from(vec![
            Span::styled("↑↓", Style::default().fg(app.theme.accent).bold()),
            Span::styled(" row · ", Style::default().fg(app.theme.dim)),
            Span::styled("←→", Style::default().fg(app.theme.accent).bold()),
            Span::styled(" change · ", Style::default().fg(app.theme.dim)),
            Span::styled("esc", Style::default().fg(app.theme.text).bold()),
            Span::styled(" close", Style::default().fg(app.theme.dim)),
        ]);
        f.render_widget(Paragraph::new(text), area);
        return;
    }
    if let Some(pick) = &app.pending_port_pick {
        let text = Line::from(vec![
            Span::styled(
                format!("1-{}", pick.ports.len()),
                Style::default().fg(app.theme.accent).bold(),
            ),
            Span::styled(" open · ", Style::default().fg(app.theme.dim)),
            Span::styled("n", Style::default().fg(app.theme.text).bold()),
            Span::styled(" cancel", Style::default().fg(app.theme.dim)),
        ]);
        f.render_widget(Paragraph::new(text), area);
        return;
    }
    let show_sort = matches!(app.focus, Focus::Processes);
    let show_kill = matches!(app.focus, Focus::Processes | Focus::Localhost);
    // On localhost it opens a URL; on sessions it jumps to the terminal the
    // session is running in. Same verb, same key.
    // On sessions the key only appears when the host can actually put that
    // tab in front. Offering it otherwise means a key that does nothing, or
    // an app brought forward that whirr is already inside.
    let show_open = match app.focus {
        Focus::Localhost => true,
        Focus::Sessions => app.selected_session_is_jumpable(),
        _ => false,
    };
    let items: [Option<&str>; 7] = [
        Some("↑↓ select"),
        show_sort.then_some("c/m sort"),
        show_open.then_some("o open"),
        show_kill.then_some("k kill"),
        Some("s settings"),
        Some("tab focus"),
        Some("q quit"),
    ];
    // Shed hints rather than let the row overflow and clip. The panels
    // already drop out in priority order as space gets tight; this is the
    // same rule applied to the footer, and it exists because adding
    // "s settings" pushed the row past 60 columns and silently cut "q quit"
    // off the end — the one key nobody should have to guess.
    //
    // Dropped first to last: settings (discoverable elsewhere), select (the
    // arrow keys are a safe guess), sort, then the card-specific actions.
    // "tab focus" and "q quit" are never dropped.
    const DROP_ORDER: [&str; 5] = ["s settings", "↑↓ select", "c/m sort", "o open", "k kill"];
    let mut shown: Vec<&str> = items.into_iter().flatten().collect();
    for victim in DROP_ORDER {
        let width: usize =
            shown.iter().map(|i| i.chars().count()).sum::<usize>() + 3 * shown.len().saturating_sub(1);
        if width <= area.width as usize {
            break;
        }
        shown.retain(|i| *i != victim);
    }
    let text = shown.join(" · ");
    let keys_width = text.chars().count();
    f.render_widget(Paragraph::new(text).style(Style::default().fg(app.theme.dim)), area);

    // The update notice shares this row, right-aligned, and only when it can
    // do so without touching the keys. The keys are the working UI; a notice
    // nobody asked for does not get to push them off the screen.
    if let Some(update) = &app.update {
        let notice = format!("↻ {} available · {}", update.latest, update.hint);
        let width = notice.chars().count();
        // One column of breathing room between the two.
        if area.width as usize >= keys_width + width + 2 {
            f.render_widget(
                Paragraph::new(notice)
                    .style(Style::default().fg(app.theme.amber))
                    .alignment(Alignment::Right),
                area,
            );
        }
    }
}
