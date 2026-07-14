use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

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

    let mut spans = Vec::new();
    for (i, p) in slow.ports.iter().enumerate() {
        let selected = focused && i == app.selected;
        let style = if selected {
            Style::default().fg(theme::BG_CELL).bg(theme::ACCENT).bold()
        } else {
            Style::default().fg(theme::ACCENT)
        };
        spans.push(Span::styled(format!(":{}", p.port), style.bold()));
        spans.push(Span::styled(
            format!(" {} ", p.process),
            if selected { style } else { Style::default().fg(theme::TEXT) },
        ));
        if i < slow.ports.len() - 1 {
            spans.push(Span::styled("· ", Style::default().fg(theme::DIM)));
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)).wrap(Wrap { trim: true }), inner);
}
