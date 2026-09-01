use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, Focus};
use crate::sampler::claude_state::Activity;
use crate::sampler::ports::{PortGroup, PortRow};
use super::text::{pad, trunc, width as disp_width};


/// Fixed field widths shared by the width budgeting below. `LABEL_WIDTH` is
/// the desired (not guaranteed — see the localhost/other branch) label
/// column; `CPU_WIDTH` is `"{:>5.1}%"` (or the `"—"` fallback formatted to
/// match); `CLAUDE_PORT_PREFIX_WIDTH` is `" :{:<6}"`.
const LABEL_WIDTH: usize = 20;
const CPU_WIDTH: usize = 6;
const CLAUDE_PORT_PREFIX_WIDTH: usize = 8;

/// Which port rows a card carries.
///
/// `Localhost` and `Others` are two of the three side-by-side cards wide
/// terminals get; `Combined` is the single card narrower ones get instead,
/// carrying all three groups at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Card {
    Localhost,
    Others,
    Combined,
}

impl Card {
    fn title(self) -> &'static str {
        match self {
            Card::Localhost => "localhost",
            Card::Others => "others",
            Card::Combined => "Ports",
        }
    }

    /// The focus whose cursor selects in this card.
    ///
    /// `Combined` answers to `Focus::Localhost` because those are the only
    /// rows it lets you select: the claude rows it displays are listening
    /// sockets, a different list from the process-sourced one
    /// `Focus::Sessions` addresses, and system agents are deliberately not
    /// actionable (`App::on_key` applies the same rule to `k`).
    fn focus(self) -> Focus {
        match self {
            Card::Localhost | Card::Combined => Focus::Localhost,
            Card::Others => Focus::Others,
        }
    }

    fn rows(self, app: &App) -> Vec<&PortRow> {
        match self {
            Card::Localhost => app.localhost_rows(),
            Card::Others => app.other_rows(),
            Card::Combined => {
                app.slow.as_ref().map(|s| s.rows.iter().collect()).unwrap_or_default()
            }
        }
    }

    /// Whether rows need to carry their group. Only the combined card holds
    /// more than one group; the single-group cards are labelled by their own
    /// title, so a marker or header there would just be noise.
    fn labels_groups(self) -> bool {
        self == Card::Combined
    }
}

/// Join `ports` as `:NNNN` tokens, never severing a number mid-digit. If they
/// all fit inside `max_width` display cells, that's the whole string;
/// otherwise only whole tokens that fit within `max_width - 1` cells are
/// kept, and a trailing ellipsis marks the cut instead of a severed number.
/// The ellipsis is emitted whenever any token is dropped — even every token,
/// at `max_width == 0` — because a row with hidden ports and no mark of it
/// looks like a process listening on nothing, which is worse than a mark
/// that overflows its allotted cell.
fn ports_str(ports: &[u16], max_width: usize) -> String {
    let tokens: Vec<String> = ports.iter().map(|p| format!(":{p}")).collect();
    let joined = tokens.join(" ");
    if disp_width(&joined) <= max_width {
        return joined;
    }
    let budget = max_width.saturating_sub(1);
    let mut out = String::new();
    for token in &tokens {
        let sep = usize::from(!out.is_empty());
        if disp_width(&out) + sep + disp_width(token) > budget {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(token);
    }
    out.push('…');
    out
}

/// How many lines rendering `rows[start..=end]` would cost: one per row,
/// plus one per group transition inside the range — the first row's group
/// always counts as a transition, since it starts a fresh header. Headers
/// cost nothing when disabled (compact tier).
fn range_cost(rows: &[&PortRow], start: usize, end: usize, headers: bool) -> usize {
    let slice = &rows[start..=end];
    if !headers {
        return slice.len();
    }
    let mut cost = 0;
    let mut last: Option<PortGroup> = None;
    for r in slice {
        if last != Some(r.group) {
            cost += 1;
        }
        last = Some(r.group);
        cost += 1;
    }
    cost
}

/// Render one ports card. Everything that used to be threaded through as a
/// pile of parameters — title, row set, which focus selects here, whether
/// rows carry their group — is a property of `card`.
pub fn render(f: &mut Frame, area: Rect, app: &App, card: Card) {
    let rows = card.rows(app);
    let rows = &rows[..];
    let focused = app.focus == card.focus();

    let stale = app.slow.as_ref().is_some_and(|s| s.stale);
    let title = card.title();
    let full_title = if stale { format!("{title} ⟳ stale") } else { title.to_string() };
    let block = app.theme.panel_block(&full_title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.slow.is_none() {
        f.render_widget(Paragraph::new("scanning…").style(Style::default().fg(app.theme.dim)), inner);
        return;
    }
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new("no listening ports").style(Style::default().fg(app.theme.dim)),
            inner,
        );
        return;
    }

    // A group can be carried by a header line or by a per-row marker. Headers
    // read better but cost a row each, which the compact tier cannot spare —
    // 8 rows of inner height is the threshold, below which the 80x24 layout
    // gives this card only two content rows.
    let headers = card.labels_groups() && inner.height >= 8;
    let markers = card.labels_groups() && !headers;
    let visible_rows = inner.height as usize;
    let cursor = focused.then(|| app.selected());
    // Rows here are not all one line tall — each group header costs an extra
    // line — so how far to scroll can't be computed from indices alone.
    let offset = super::scroll::offset_while(cursor, |from| {
        range_cost(rows, from, cursor.expect("only called when focused"), headers) <= visible_rows
    });

    let mut lines: Vec<Line> = Vec::new();
    let mut last_group: Option<PortGroup> = None;
    for (i, r) in rows.iter().copied().enumerate().skip(offset) {
        if lines.len() >= visible_rows {
            break;
        }
        if headers && last_group != Some(r.group) {
            lines.push(Line::styled(
                match r.group {
                    PortGroup::Localhost => "localhost",
                    PortGroup::Claude => "claude sessions",
                    PortGroup::Other => "others",
                },
                Style::default().fg(app.theme.dim),
            ));
            last_group = Some(r.group);
            if lines.len() >= visible_rows {
                break;
            }
        }
        let selected = cursor == Some(i);
        let base = if selected {
            Style::default().fg(app.theme.bg_cell).bg(app.theme.accent)
        } else {
            Style::default().fg(app.theme.text)
        };
        let mut spans: Vec<Span> = Vec::new();
        let mut prefix_width = 0usize;
        if markers {
            // The marker carries the group when there is no header to do it.
            let (glyph, colour) = match r.group {
                PortGroup::Localhost => ("●", app.theme.accent),
                // A Claude row's marker is its state, not its group: the same
                // shape and the same colour the sessions card uses one tier
                // up. The group is already legible from where the row sits.
                PortGroup::Claude => claude_marker(app, r.pid),
                PortGroup::Other => ("○", app.theme.dim),
            };
            spans.push(Span::styled(glyph, if selected { base } else { Style::default().fg(colour) }));
            prefix_width += disp_width(glyph);
        }
        let total = inner.width as usize;
        match r.group {
            PortGroup::Claude => {
                // Identity plus "is it working" — the port is the least useful
                // column for an ephemeral session port, so it's shown first,
                // and only when there is width for headers. Budget the whole
                // row against `total` before committing to any of it: drop
                // the port prefix first, then the CPU column entirely, rather
                // than let either get clipped mid-value — a dropped column
                // is honest, a severed number is not.
                // Leading space + label, plus the state glyph and its space
                // when the row carries one. The glyph is never dropped: it is
                // two cells, and it is the only column that says whether this
                // session is running without you.
                let label_field_width = 1 + LABEL_WIDTH + if headers { 2 } else { 0 };
                let port_prefix_width = if headers { CLAUDE_PORT_PREFIX_WIDTH } else { 0 };
                let mut show_port_prefix = headers;
                let mut show_cpu = true;
                if prefix_width + port_prefix_width + label_field_width + CPU_WIDTH > total {
                    show_port_prefix = false;
                }
                if prefix_width + label_field_width + CPU_WIDTH > total {
                    show_cpu = false;
                }
                if show_port_prefix {
                    spans.push(Span::styled(format!(" :{:<6}", r.ports[0]), base.bold()));
                }
                // With headers there is no marker column, so the state glyph
                // rides the row itself. Its two cells were budgeted above.
                if headers {
                    let (glyph, colour) = claude_marker(app, r.pid);
                    spans.push(Span::styled(
                        format!(" {glyph}"),
                        if selected { base } else { Style::default().fg(colour) },
                    ));
                }
                spans.push(Span::styled(
                    format!(" {}", pad(&trunc(&r.label, LABEL_WIDTH), LABEL_WIDTH)),
                    base,
                ));
                if show_cpu {
                    let cpu_text = match app.cpu_of(r.pid) {
                        Some(cpu) => format!("{cpu:>5.1}%"),
                        None => format!("{:>6}", "—"),
                    };
                    spans.push(Span::styled(
                        cpu_text,
                        if selected { base } else { Style::default().fg(app.theme.accent) },
                    ));
                }
            }
            _ => {
                // Shrink the label field, if need be, to guarantee at least
                // one cell for the port list — enough for `ports_str` to
                // show its ellipsis instead of the row silently claiming
                // zero ports.
                let avail_for_label_and_ports = total.saturating_sub(prefix_width + 1);
                let label_width = LABEL_WIDTH.min(avail_for_label_and_ports.saturating_sub(1));
                let truncated_label = trunc(&r.label, label_width);
                let label_was_truncated = truncated_label.ends_with('…');
                spans.push(Span::styled(
                    format!(" {}", pad(&truncated_label, label_width)),
                    base,
                ));
                let used: usize = spans.iter().map(|s| disp_width(&s.content)).sum();
                let avail = total.saturating_sub(used);
                let mut ports_output = ports_str(&r.ports, avail);
                // Suppress the ports ellipsis if the label was already truncated,
                // to avoid doubled ellipsis at narrow widths.
                if label_was_truncated && ports_output.ends_with('…') {
                    ports_output.pop();
                }
                spans.push(Span::styled(
                    ports_output,
                    if selected { base } else { Style::default().fg(app.theme.accent) },
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);
}


/// The shape and colour a Claude row wears, joined on by pid so the grouped
/// card and the sessions card say the same thing about the same session.
///
/// A pid with no state — a port-sourced row whose process the session scan
/// did not match — keeps the hollow shape rather than inventing one.
fn claude_marker(app: &App, pid: i32) -> (&'static str, ratatui::style::Color) {
    let Some(st) = app.session_state(pid) else { return ("○", app.theme.text) };
    let colour = if st.warn {
        app.theme.amber
    } else if matches!(st.activity, Activity::Busy) {
        app.theme.accent
    } else {
        app.theme.text
    };
    (super::sessions::glyph(&st.activity), colour)
}

#[cfg(test)]
mod tests {
    /// The palette the app starts with; tests assert against it by name
    /// rather than against raw RGB triples.
    const TH: crate::ui::theme::Theme = crate::ui::theme::Theme::dark();

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use std::time::Duration;

    use crate::app::{App, Focus};
    use crate::sampler::claude_state::{Activity, SessionState};
    use crate::sampler::ports::{PortGroup, PortRow};
    use crate::sampler::sessions::ClaudeSession;
    use crate::sampler::{SlowSnap, Snapshot};

    /// Render a given `App`'s ports card at a given size.
    fn draw_app(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::render(f, f.area(), app, super::Card::Combined)).unwrap();
        let b = t.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| b[(x, y)].symbol().to_string()).collect())
            .collect()
    }

    /// Render just the ports card at a given size, using the demo app.
    fn draw(w: u16, h: u16) -> Vec<String> {
        draw_app(&App::demo(), w, h)
    }

    /// A focused ports card with `rows` and a given selection, bypassing
    /// `App::demo()`'s five rows (too few to reproduce a scroll).
    fn app_with_rows(rows: Vec<PortRow>, selected: usize) -> App {
        let mut app = App::new(false);
        app.focus = Focus::Localhost;
        app.ingest(Snapshot::Slow(SlowSnap { rows, sessions: Vec::new(), stale: false }));
        app.select(selected);
        app
    }

    /// A card holding one Claude row and the session it belongs to, joined by
    /// pid the way the real snapshot joins them.
    fn app_with_session(activity: Activity, warn: bool) -> App {
        let mut app = App::new(false);
        app.ingest(Snapshot::Slow(SlowSnap {
            rows: vec![PortRow {
                group: PortGroup::Claude,
                label: "axterio".into(),
                pid: 700,
                ports: vec![65067],
            }],
            sessions: vec![ClaudeSession {
                pid: 700,
                project: "axterio".into(),
                title: None,
                jumpable: false,
                tty: Some("ttys020".into()),
                state: SessionState { activity, subagents: Vec::new(), shell: None, writing_age: None, warn },
                about: Default::default(),
            }],
            stale: false,
        }));
        app
    }

    /// 10 localhost rows, 4 claude rows, 2 other rows. Only the localhost
    /// group is addressable by the cursor (`Focus::Localhost` is the only
    /// focus this card answers to), so the scrollable range has to live
    /// there; the other two groups are present so their headers still charge
    /// against the height budget.
    fn scroll_test_rows() -> Vec<PortRow> {
        let mut rows = Vec::new();
        for i in 0..10i32 {
            rows.push(PortRow {
                group: PortGroup::Localhost,
                label: format!("local{i}"),
                pid: 100 + i,
                ports: vec![3000 + i as u16],
            });
        }
        for i in 0..4i32 {
            rows.push(PortRow {
                group: PortGroup::Claude,
                label: format!("claude{i}"),
                pid: 200 + i,
                ports: vec![4000 + i as u16],
            });
        }
        for i in 0..2i32 {
            rows.push(PortRow {
                group: PortGroup::Other,
                label: format!("other{i}"),
                pid: 300 + i,
                ports: vec![5000 + i as u16],
            });
        }
        rows
    }

    #[test]
    fn full_tier_shows_group_headers_in_order() {
        let rows = draw(46, 14).join("\n");
        let local = rows.find("localhost").expect("localhost header missing");
        let claude = rows.find("claude").expect("claude header missing");
        let other = rows.find("other").expect("others header missing");
        // Exact match, not substring — "other" is a substring of "others" so
        // `.find("other")` alone can't tell them apart.
        assert_eq!(&rows[other..other + "others".len()], "others", "header must read \"others\"");
        assert!(local < claude && claude < other, "headers must be localhost, claude, others");
    }

    #[test]
    fn a_process_with_three_ports_renders_them_on_one_line() {
        let rows = draw(46, 14);
        let line = rows
            .iter()
            .find(|l| l.contains("glassbook-frontend"))
            .expect("localhost row missing");
        assert!(line.contains("4206"), "first port missing: {line}");
        assert!(line.contains("6006"), "second port missing: {line}");
    }

    #[test]
    fn claude_rows_show_live_cpu() {
        let rows = draw(46, 14).join("\n");
        // demo() gives pid 503 (claude/axterio) 12.4% CPU.
        assert!(rows.contains("12.4"), "claude row should show its CPU");
    }

    #[test]
    fn scroll_offset_keeps_selection_visible() {
        // Counterexample from the review: with headers on, the old
        // index-based offset (`selected - (visible_rows - 1)`) undercounts
        // how far to scroll because it never charges for header lines. Cover
        // a spread of budgets and selections, including the very last row,
        // and require the selected row's own label to actually be on
        // screen — not just that *some* row rendered. Covers 4..7 (compact,
        // no headers) as well as 8..14 (full tier, headers on) — the
        // original range only covered the headers-on path.
        let rows = scroll_test_rows();
        for visible_rows in 4u16..=14 {
            for &selected in &[0usize, 3, 4, 7, 8, 9] {
                // Every index here is inside the localhost group — the only
                // rows this card's cursor can address.
                let app = app_with_rows(rows.clone(), selected);
                let out = draw_app(&app, 46, visible_rows + 2).join("\n");
                let label = &rows[selected].label;
                assert!(
                    out.contains(label.as_str()),
                    "visible_rows={visible_rows} selected={selected}: label {label:?} not on screen:\n{out}"
                );
            }
        }
    }

    #[test]
    fn compact_tier_drops_headers_and_uses_markers() {
        // 6 rows of inner height is the compact case.
        let rows = draw(46, 6).join("\n");
        assert!(!rows.contains("localhost"), "compact must not render group headers");
        assert!(rows.contains('●'), "compact needs a localhost marker");
    }

    #[test]
    fn single_group_card_has_no_markers() {
        // Single-group cards (localhost/others) should never render group markers
        // because the card's title already identifies the group.
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let rows = vec![PortRow {
            group: PortGroup::Localhost,
            label: "web".into(),
            pid: 601,
            ports: vec![3000],
        }];
        let app = app_with_rows(rows, 0);

        // Test render_localhost at compact height (6 rows)
        let mut t = Terminal::new(TestBackend::new(46, 6)).unwrap();
        t.draw(|f| super::render(f, f.area(), &app, super::Card::Localhost)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut text = String::new();
        for y in 0..6u16 {
            for x in 0..46u16 {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(
            !text.contains('●') && !text.contains("○"),
            "single-group card should not render markers: {text}"
        );
    }

    /// Every `:NNNN` token in `line`, in order — a severed port would show
    /// up here as a number that doesn't match any of the row's real ports.
    fn port_tokens(line: &str) -> Vec<u16> {
        let chars: Vec<char> = line.chars().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == ':' {
                let mut j = i + 1;
                let mut digits = String::new();
                while j < chars.len() && chars[j].is_ascii_digit() {
                    digits.push(chars[j]);
                    j += 1;
                }
                if let Ok(n) = digits.parse::<u16>() {
                    out.push(n);
                }
                i = j.max(i + 1);
                continue;
            }
            i += 1;
        }
        out
    }

    #[test]
    fn ports_render_in_full_when_they_fit() {
        // At 46 columns every demo row's ports fit with room to spare, at
        // both tiers. Assert the *complete* port set is present, not just
        // that a substring happens to appear somewhere on the line — that's
        // what let a mid-value cut through the old test unnoticed. Rows are
        // matched to lines positionally (not by a label search) because two
        // demo rows share the label "axterio" — one localhost, one claude.
        for h in [14u16, 6] {
            let headers = h >= 8;
            let text = draw(46, h);
            let demo_rows = App::demo().slow.unwrap().rows;
            let mut idx = 0;
            for line in &text {
                if idx >= demo_rows.len() {
                    break;
                }
                let row = &demo_rows[idx];
                if !line.contains(row.label.as_str()) {
                    continue; // header, border, or blank padding line
                }
                let expected: Vec<u16> = match row.group {
                    PortGroup::Claude if headers => vec![row.ports[0]],
                    PortGroup::Claude => vec![],
                    _ => row.ports.clone(),
                };
                assert_eq!(
                    port_tokens(line),
                    expected,
                    "row {:?} at h={h} (headers={headers}) missing/extra ports: {line:?}",
                    row.label
                );
                idx += 1;
            }
        }
    }

    #[test]
    fn ports_that_cannot_fit_end_in_ellipsis_not_a_severed_number() {
        // A row whose ports can never fit, even generously. The renderer
        // must drop whole tokens and mark the cut with an ellipsis rather
        // than emit a partial number — that's the deliberate behaviour
        // being pinned here.
        let rows = vec![PortRow {
            group: PortGroup::Other,
            label: "many".into(),
            pid: 900,
            ports: (0..16).map(|i| 40000 + i as u16).collect(),
        }];
        let app = app_with_rows(rows.clone(), 0);
        let text = draw_app(&app, 34, 8); // compact tier, room for one port plus ellipsis
        let line = text.iter().find(|l| l.contains("many")).expect("row missing from output");

        let shown = port_tokens(line);
        assert!(shown.len() < rows[0].ports.len(), "expected a cut, but everything fit: {line:?}");
        for p in &shown {
            assert!(
                rows[0].ports.contains(p),
                "port {p} isn't one of the row's real ports — looks like a severed number: {line:?}"
            );
        }
        assert!(
            line.trim_end_matches(['│', ' ']).ends_with('…'),
            "a line that can't fit its ports should end in an ellipsis: {line:?}"
        );
    }

    /// One row per group, sized to fit comfortably at 46 columns — used to
    /// exercise the compact claude/other branches that
    /// `ports_render_in_full_when_they_fit`'s 4-content-row demo card never
    /// reaches.
    fn one_row_per_group() -> Vec<PortRow> {
        vec![
            PortRow { group: PortGroup::Localhost, label: "web".into(), pid: 601, ports: vec![3000] },
            PortRow { group: PortGroup::Claude, label: "claude-proj".into(), pid: 602, ports: vec![4001] },
            PortRow { group: PortGroup::Other, label: "syslogd".into(), pid: 603, ports: vec![5001] },
        ]
    }

    #[test]
    fn compact_claude_row_shows_cpu_and_drops_its_port() {
        // The 4-content-row demo card at h=6 only ever shows localhost rows
        // (see `ports_render_in_full_when_they_fit`'s comment), so the
        // compact claude branch has never actually been rendered by a test.
        let app = app_with_rows(one_row_per_group(), 0);
        let text = draw_app(&app, 46, 6);
        let line = text.iter().find(|l| l.contains("claude-proj")).expect("claude row missing");
        // CPU field should be present: either "12.4%" for known CPU or "—" for unknown
        assert!(line.contains('%') || line.contains('—'), "compact claude row should show its CPU: {line}");
        assert!(
            port_tokens(line).is_empty(),
            "compact claude row must not show its port (full tier only): {line}"
        );
    }

    #[test]
    fn compact_other_row_renders_its_ports() {
        let app = app_with_rows(one_row_per_group(), 0);
        let text = draw_app(&app, 46, 6);
        let line = text.iter().find(|l| l.contains("syslogd")).expect("other row missing");
        assert_eq!(port_tokens(line), vec![5001], "other row should render its port: {line}");
    }

    #[test]
    fn compact_markers_map_to_the_correct_group_colour() {
        // `compact_tier_drops_headers_and_uses_markers` only checks that a
        // '●' appears somewhere — it can't tell claude's '○' apart from
        // other's '○'. Check the actual cell colour at each marker. Leave the
        // card unfocused so nothing is highlighted — a selected marker's
        // colour is overridden by the highlight style, not its group colour.
        let mut app = app_with_rows(one_row_per_group(), 0);
        app.focus = Focus::Processes;
        let mut t = Terminal::new(TestBackend::new(46, 6)).unwrap();
        t.draw(|f| super::render(f, f.area(), &app, super::Card::Combined)).unwrap();
        let buf = t.backend().buffer().clone();
        let lines: Vec<String> =
            (0..6u16).map(|y| (0..46u16).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect();

        let row_y = |needle: &str| {
            lines.iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("{needle} row missing")) as u16
        };
        let marker_fg = |y: u16| {
            (0..46u16)
                .find_map(|x| {
                    let cell = &buf[(x, y)];
                    matches!(cell.symbol(), "●" | "○").then(|| cell.style().fg.unwrap())
                })
                .unwrap_or_else(|| panic!("no marker glyph found on row {y}"))
        };

        assert_eq!(marker_fg(row_y("web")), TH.accent, "localhost marker should be ACCENT");
        assert_eq!(marker_fg(row_y("claude-proj")), TH.text, "claude marker should be TEXT");
        assert_eq!(marker_fg(row_y("syslogd")), TH.dim, "other marker should be DIM");
    }

    #[test]
    fn claude_row_shows_dash_for_unknown_cpu_not_a_false_zero() {
        // demo()'s pid 504 (ai-design-kit) has no matching fast-snapshot
        // process, so its CPU is unknown. `unwrap_or(0.0)` used to render
        // that as a confident "0.0%" — indistinguishable from an idle
        // process that really is at 0%.
        let text = draw(46, 14).join("\n");
        let line = text.lines().find(|l| l.contains("ai-design-kit")).expect("row missing");
        // Extract the CPU field: it's the rightmost 6-char field before any border
        let trimmed = line.trim_end_matches(['│', ' ']);
        let cpu_field = if trimmed.len() >= 6 {
            &trimmed[trimmed.len() - 6..]
        } else {
            trimmed
        };
        assert!(cpu_field.contains('—'), "unknown CPU should render as —: {cpu_field:?} from {line}");
        assert!(!cpu_field.contains('%'), "unknown CPU field must not contain %: {cpu_field:?} from {line}");
    }

    #[test]
    fn claude_cpu_column_is_never_severed_at_odd_terminal_sizes() {
        // Reviewer-verified regression: the claude row used to render a
        // fixed ~35-column layout with no width check, so ratatui clipped
        // the CPU value mid-digit — 57x40 showed "12", 60x40 showed "12.4"
        // with no "%". The column must now be all-or-nothing: the whole
        // "12.4%" or nothing, never a partial number.
        for (w, h) in [(57u16, 40u16), (60u16, 40u16)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| crate::ui::draw(f, &App::demo())).unwrap();
            let buf = t.backend().buffer().clone();
            let lines: Vec<String> =
                (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect();
            let header_idx = lines
                .iter()
                .position(|l| l.contains("claude sessions"))
                .unwrap_or_else(|| panic!("{w}x{h}: claude sessions header missing"));
            // demo()'s first claude row (pid 503, "axterio", 12.4% CPU)
            // immediately follows its group header.
            let axterio_line = &lines[header_idx + 1];
            let suffix = axterio_line.split("axterio").last().unwrap();
            let trimmed = suffix.trim_end_matches(['│', ' ']).trim();
            assert!(
                trimmed.is_empty() || trimmed == "12.4%",
                "{w}x{h}: claude CPU severed instead of shown whole or dropped: {trimmed:?} in {axterio_line:?}"
            );
        }
    }

    #[test]
    fn compact_claude_cpu_column_is_dropped_whole_when_it_cannot_fit() {
        // Reviewer-verified regression: compact 28x6 rendered "12."; 26x6
        // rendered "1". At 26 columns there's no room for the full "12.4%",
        // so it must be dropped entirely rather than clipped.
        let text = draw(26, 6).join("\n");
        let line = text.lines().find(|l| l.contains("axterio")).expect("claude row missing");
        assert!(!line.contains('%'), "CPU should be dropped whole, not clipped: {line:?}");
    }

    #[test]
    fn cjk_label_width_is_measured_in_cells_not_chars() {
        // Reviewer-verified regression: counting chars instead of display
        // cells overstated remaining width for a CJK label, so the terminal
        // clipped the port list *after* `ports_str` had already decided
        // everything fit and appended nothing — label
        // "项目名称测试项目名称" (10 chars, 20 cells) with ports
        // 3000/3001/3002/63643 at 46 columns rendered ":3000 :3001" with no
        // ellipsis.
        let rows = vec![PortRow {
            group: PortGroup::Localhost,
            label: "项目名称测试项目名称".into(),
            pid: 900,
            ports: vec![3000, 3001, 3002, 63643],
        }];
        let app = app_with_rows(rows.clone(), 0);
        let text = draw_app(&app, 46, 6);
        let line = text.iter().find(|l| l.contains('项')).expect("row missing");

        let shown = port_tokens(line);
        assert!(shown.len() < rows[0].ports.len(), "expected a cut, but everything fit: {line:?}");
        for p in &shown {
            assert!(
                rows[0].ports.contains(p),
                "port {p} isn't one of the row's real ports — looks like a severed number: {line:?}"
            );
        }
        assert!(
            line.trim_end_matches(['│', ' ']).ends_with('…'),
            "a cut CJK-labelled port list must end in an ellipsis: {line:?}"
        );
    }

    #[test]
    fn extremely_narrow_rows_still_signal_a_cut_with_ellipsis() {
        // Reviewer-verified regression: at 22x6 the localhost row's 3 ports
        // vanished with no `…` — a process with hidden ports rendered
        // indistinguishably from one listening on nothing. At 18x6 the
        // label itself was severed ("glassbook-fron") with no ellipsis
        // either.
        for (w, h) in [(22u16, 6u16), (18u16, 6u16)] {
            let text = draw(w, h);
            let line = text.iter().find(|l| l.contains("glassbook")).expect("row missing");
            let trimmed = line.trim_end_matches(['│', ' ']);
            assert!(
                trimmed.ends_with('…'),
                "{w}x{h}: a row that can't show its full label/ports must end in an ellipsis: {line:?}"
            );
        }
    }

    #[test]
    fn no_doubled_ellipsis_when_both_label_and_ports_truncated() {
        // At 22 columns, both the label truncation and the port list
        // truncation would independently append ellipsis, producing "……" which
        // looks like a rendering glitch. When the label was already truncated,
        // suppress the ports ellipsis to keep just one.
        let text = draw(22, 6);
        let line = text.iter().find(|l| l.contains("glassbook")).expect("row missing");
        let content = line.trim_end_matches(['│', ' ']);
        let ellipsis_count = content.matches('…').count();
        assert_eq!(
            ellipsis_count, 1,
            "row with both label and ports truncated must have exactly one ellipsis: {line:?}"
        );
    }


    #[test]
    fn the_grouped_card_wears_the_same_shapes_as_the_sessions_card() {
        // Below the width that earns three cards, sessions live inside the
        // grouped ports card. Reading the same shape in both places is the
        // difference between one design at every size and two.
        for (activity, shape) in [
            (Activity::Busy, '●'),
            (Activity::Loop { wakes_in: Duration::from_secs(60) }, '◐'),
            (Activity::BgJob, '◐'),
            (Activity::Idle { since: None }, '○'),
        ] {
            let app = app_with_session(activity.clone(), false);
            let out = draw_app(&app, 60, 10).join("\n");
            let row = out.lines().find(|l| l.contains("axterio")).expect("the row is drawn");
            assert!(row.contains(shape), "{activity:?} should wear {shape}: {row:?}");
        }
    }

    #[test]
    fn a_claude_row_with_no_session_behind_it_keeps_the_hollow_shape() {
        // A port-sourced row whose process the session scan never matched.
        // Inventing a state for it would be worse than saying nothing.
        let app = app_with_rows(
            vec![PortRow { group: PortGroup::Claude, label: "orphan".into(), pid: 900, ports: vec![65000] }],
            0,
        );
        let out = draw_app(&app, 60, 10).join("\n");
        let row = out.lines().find(|l| l.contains("orphan")).expect("the row is drawn");
        assert!(row.contains('○'), "an unknown session stays hollow: {row:?}");
    }
}
