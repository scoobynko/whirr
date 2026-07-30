use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, Focus};
use crate::sampler::ports::{PortGroup, PortRow};
use super::theme;

/// Clip to `max` chars without splitting a char boundary.
fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).chain(['…']).collect()
    }
}

/// Join `ports` as `:NNNN` tokens, never severing a number mid-digit. If they
/// all fit inside `max_width`, that's the whole string; otherwise only whole
/// tokens that fit within `max_width - 1` are kept, and a trailing ellipsis
/// (always within budget) marks the cut instead of a severed number.
fn ports_str(ports: &[u16], max_width: usize) -> String {
    let tokens: Vec<String> = ports.iter().map(|p| format!(":{p}")).collect();
    let joined = tokens.join(" ");
    if joined.chars().count() <= max_width {
        return joined;
    }
    let budget = max_width.saturating_sub(1);
    let mut out = String::new();
    for token in &tokens {
        let sep = usize::from(!out.is_empty());
        if out.chars().count() + sep + token.chars().count() > budget {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(token);
    }
    if max_width >= 1 {
        out.push('…');
    }
    out
}

/// How many lines rendering `rows[start..=end]` would cost: one per row,
/// plus one per group transition inside the range — the first row's group
/// always counts as a transition, since it starts a fresh header. Headers
/// cost nothing when disabled (compact tier).
fn range_cost(rows: &[PortRow], start: usize, end: usize, headers: bool) -> usize {
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

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let focused = matches!(app.focus, Focus::Ports);
    let stale = app.slow.as_ref().is_some_and(|s| s.stale);
    let title = if stale { "Ports ⟳ stale" } else { "Ports" };
    let block = theme::panel_block(title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(slow) = app.slow.as_ref() else {
        f.render_widget(Paragraph::new("scanning…").style(Style::default().fg(theme::DIM)), inner);
        return;
    };
    if slow.rows.is_empty() {
        f.render_widget(
            Paragraph::new("no listening ports").style(Style::default().fg(theme::DIM)),
            inner,
        );
        return;
    }

    // Headers cost a row each, which the compact tier cannot spare; there it
    // uses a per-row marker instead. 8 rows of inner height is the threshold —
    // below that, the 80x24 layout gives this card only two content rows.
    let headers = inner.height >= 8;
    let visible_rows = inner.height as usize;
    let offset = if focused {
        let selected = app.selected.min(slow.rows.len().saturating_sub(1));
        let mut offset = 0;
        while offset < selected && range_cost(&slow.rows, offset, selected, headers) > visible_rows {
            offset += 1;
        }
        offset
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();
    let mut last_group: Option<PortGroup> = None;
    for (i, r) in slow.rows.iter().enumerate().skip(offset) {
        if lines.len() >= visible_rows {
            break;
        }
        if headers && last_group != Some(r.group) {
            lines.push(Line::styled(
                match r.group {
                    PortGroup::Localhost => "localhost",
                    PortGroup::Claude => "claude sessions",
                    PortGroup::Other => "other",
                },
                Style::default().fg(theme::DIM),
            ));
            last_group = Some(r.group);
            if lines.len() >= visible_rows {
                break;
            }
        }
        let selected = focused && i == app.selected;
        let base = if selected {
            Style::default().fg(theme::BG_CELL).bg(theme::ACCENT)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let mut spans: Vec<Span> = Vec::new();
        if !headers {
            // Marker carries the group when there is no header to do it.
            let (glyph, colour) = match r.group {
                PortGroup::Localhost => ("●", theme::ACCENT),
                PortGroup::Claude => ("○", theme::TEXT),
                PortGroup::Other => ("○", theme::DIM),
            };
            spans.push(Span::styled(glyph, if selected { base } else { Style::default().fg(colour) }));
        }
        spans.push(Span::styled(format!(" {:<20}", trunc(&r.label, 20)), base));
        match r.group {
            PortGroup::Claude => {
                // Identity plus "is it working" — the port is the least useful
                // column for an ephemeral session port, so it goes first only
                // when there is width for headers.
                if headers {
                    spans.insert(0, Span::styled(format!(" :{:<6}", r.ports[0]), base.bold()));
                }
                let cpu = app.cpu_of(r.pid).unwrap_or(0.0);
                spans.push(Span::styled(
                    format!("{cpu:>5.1}%"),
                    if selected { base } else { Style::default().fg(theme::ACCENT) },
                ));
            }
            _ => {
                let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                let avail = (inner.width as usize).saturating_sub(used);
                spans.push(Span::styled(
                    ports_str(&r.ports, avail),
                    if selected { base } else { Style::default().fg(theme::ACCENT) },
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::{App, Focus};
    use crate::sampler::ports::{PortGroup, PortRow};
    use crate::sampler::{SlowSnap, Snapshot};

    /// Render a given `App`'s ports card at a given size.
    fn draw_app(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::render(f, f.area(), app)).unwrap();
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
        app.focus = Focus::Ports;
        app.ingest(Snapshot::Slow(SlowSnap { rows, stale: false }));
        app.selected = selected;
        app
    }

    /// 4 localhost rows, 4 claude rows, 2 other rows — spans multiple
    /// groups and outnumbers any reasonable `visible_rows` budget, which is
    /// what it takes to reproduce the header/offset mismatch in finding 1.
    fn scroll_test_rows() -> Vec<PortRow> {
        let mut rows = Vec::new();
        for i in 0..4i32 {
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
        // screen — not just that *some* row rendered.
        let rows = scroll_test_rows();
        for visible_rows in 8u16..=14 {
            for &selected in &[0usize, 3, 4, 7, 8, 9] {
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
}
