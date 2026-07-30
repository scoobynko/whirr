use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, Focus};
use crate::sampler::ports::PortGroup;
use super::theme;

/// Clip to `max` chars without splitting a char boundary.
fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).chain(['…']).collect()
    }
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
        app.selected.saturating_sub(visible_rows.saturating_sub(1))
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
                    spans.insert(0, Span::styled(format!(":{:<6}", r.ports[0]), base.bold()));
                }
                let cpu = app.cpu_of(r.pid).unwrap_or(0.0);
                spans.push(Span::styled(
                    format!("{cpu:>5.1}%"),
                    if selected { base } else { Style::default().fg(theme::ACCENT) },
                ));
            }
            _ => {
                let ports: Vec<String> = r.ports.iter().map(|p| format!(":{p}")).collect();
                spans.push(Span::styled(
                    ports.join(" "),
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

    use crate::app::App;

    /// Render just the ports card at a given size.
    fn draw(w: u16, h: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let b = t.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| b[(x, y)].symbol().to_string()).collect())
            .collect()
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
    fn compact_tier_drops_headers_and_uses_markers() {
        // 6 rows of inner height is the compact case.
        let rows = draw(46, 6).join("\n");
        assert!(!rows.contains("localhost"), "compact must not render group headers");
        assert!(rows.contains('●'), "compact needs a localhost marker");
    }

    #[test]
    fn nothing_truncates_mid_value_at_eighty_by_twentyfour() {
        // The ports card is ~46 cols in the 80x24 layout.
        for line in draw(46, 6) {
            // A row that ends in a partial port number is a truncation bug.
            let t = line.trim_end();
            assert!(
                !t.ends_with(':') && !t.ends_with('·'),
                "line looks cut mid-value: {t:?}"
            );
        }
    }
}
