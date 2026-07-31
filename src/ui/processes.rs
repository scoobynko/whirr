use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, Focus, SortBy};
use crate::units::fmt_bytes;
use super::theme;

pub fn micro_bar(pct: f32, width: usize) -> usize {
    ((pct / 100.0).clamp(0.0, 1.0) * width as f32).round() as usize
}

const BAR_W: usize = 8;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let focused = matches!(app.focus, Focus::Processes);
    let title = match app.sort_by {
        SortBy::Cpu => "Processes ▾cpu",
        SortBy::Mem => "Processes ▾mem",
    };
    let block = theme::panel_block(title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let procs = app.visible_processes();
    if procs.is_empty() {
        f.render_widget(Paragraph::new("…").style(Style::default().fg(theme::DIM)), inner);
        return;
    }
    // The old always-on hint row moved to the global footer (see
    // `ui::render_footer`); this status line only exists — and only claims a
    // row — when there's actually a kill confirmation or message to show.
    let status = if let Some((pid, name)) = &app.pending_kill {
        Some(Line::styled(
            format!("kill {name} ({pid})? y/n"),
            Style::default().fg(theme::RED).bold(),
        ))
    } else {
        app.message.as_ref().map(|msg| Line::styled(msg.clone(), Style::default().fg(theme::AMBER)))
    };

    let mem_total = app.fast.as_ref().map_or(1, |f| f.mem_total).max(1);
    let visible_rows = inner.height.saturating_sub(status.is_some() as u16) as usize;
    let offset = app.selected.saturating_sub(visible_rows.saturating_sub(1));

    let mut lines = Vec::new();
    for (i, p) in procs.iter().enumerate().skip(offset).take(visible_rows) {
        let selected = focused && i == app.selected;
        let base = if selected {
            Style::default().fg(theme::TEXT).bg(theme::BG_CELL).bold()
        } else if i == 0 {
            Style::default().fg(theme::TEXT).bold()
        } else {
            Style::default().fg(theme::TEXT)
        };
        let cpu_fill = micro_bar(p.cpu, BAR_W);
        let mem_pct = (p.mem as f64 / mem_total as f64 * 100.0) as f32;
        let mem_fill = micro_bar(mem_pct, BAR_W);
        let bar = |fill: usize| {
            format!("{}{}", "▮".repeat(fill), "▯".repeat(BAR_W - fill))
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:>6} ", p.pid), base.fg(theme::DIM)),
            Span::styled(format!("{:<24.24} ", p.name), base),
            Span::styled(format!("{:>5.1}% ", p.cpu), base),
            Span::styled(bar(cpu_fill), Style::default().fg(theme::gradient(p.cpu / 100.0))),
            Span::styled(format!(" {:>9} ", fmt_bytes(p.mem)), base),
            Span::styled(bar(mem_fill), Style::default().fg(theme::gradient(mem_pct / 100.0))),
        ]));
    }

    if let Some(status) = status {
        lines.push(status);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::micro_bar;

    #[test]
    fn bar_clamps_and_scales() {
        assert_eq!(micro_bar(0.0, 10), 0);
        assert_eq!(micro_bar(50.0, 10), 5);
        assert_eq!(micro_bar(100.0, 10), 10);
        assert_eq!(micro_bar(250.0, 10), 10); // multi-core > 100%
    }
}
