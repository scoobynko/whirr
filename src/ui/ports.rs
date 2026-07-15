use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, Focus};
use super::theme;

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
    if slow.ports.is_empty() {
        f.render_widget(
            Paragraph::new("no listening ports").style(Style::default().fg(theme::DIM)),
            inner,
        );
        return;
    }

    // One port per line, scrolled so the selection stays visible.
    let visible_rows = inner.height as usize;
    let offset = if focused {
        app.selected.saturating_sub(visible_rows.saturating_sub(1))
    } else {
        0
    };

    let mut lines = Vec::new();
    for (i, p) in slow.ports.iter().enumerate().skip(offset).take(visible_rows) {
        let selected = focused && i == app.selected;
        let style = if selected {
            Style::default().fg(theme::BG_CELL).bg(theme::ACCENT)
        } else {
            Style::default().fg(theme::ACCENT)
        };
        let mut spans = vec![
            Span::styled(format!(":{:<5}", p.port), style.bold()),
            Span::styled(
                format!(" {} ", p.process),
                if selected { style.bold() } else { Style::default().fg(theme::TEXT) },
            ),
        ];
        if let Some(project) = &p.project {
            spans.push(Span::styled(
                format!("({project})"),
                if selected { style } else { Style::default().fg(theme::DIM) },
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);
}
