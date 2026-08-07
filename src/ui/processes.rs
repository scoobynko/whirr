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

    let procs = app.processes();
    if procs.is_empty() {
        f.render_widget(Paragraph::new("…").style(Style::default().fg(theme::DIM)), inner);
        return;
    }
    // The old always-on hint row moved to the global footer (see
    // `ui::render_footer`); this status line only exists — and only claims a
    // row — when there's actually a message to show.
    //
    // The kill confirmation used to live here too, which is why one raised on
    // the localhost card appeared inside this panel. It is a dialog now (see
    // `ui::modal`); this line is left to `message`, which is a status report
    // and not a question.
    let status =
        app.message.as_ref().map(|msg| Line::styled(msg.clone(), Style::default().fg(theme::AMBER)));

    let mem_total = app.fast.as_ref().map_or(1, |f| f.mem_total).max(1);
    let visible_rows = inner.height.saturating_sub(status.is_some() as u16) as usize;
    let cursor = focused.then(|| app.selected());
    let offset = super::scroll::offset(visible_rows, cursor);

    let mut lines = Vec::new();
    for (i, p) in procs.iter().enumerate().skip(offset).take(visible_rows) {
        let selected = cursor == Some(i);
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
        // Both bars take their colour from `base.fg(..)` rather than a fresh
        // `Style::default().fg(..)`: `base` carries the selected row's
        // background, and a fresh style would drop it, leaving the bar cells
        // to fall through to the frame's BASE and cut two dark notches out of
        // the highlight. Same reason `pid` uses `base.fg(theme::DIM)`.
        lines.push(Line::from(vec![
            Span::styled(format!("{:>6} ", p.pid), base.fg(theme::DIM)),
            Span::styled(format!("{:<24.24} ", p.name), base),
            Span::styled(format!("{:>5.1}% ", p.cpu), base),
            Span::styled(bar(cpu_fill), base.fg(theme::gradient(p.cpu / 100.0))),
            Span::styled(format!(" {:>9} ", fmt_bytes(p.mem)), base),
            Span::styled(bar(mem_fill), base.fg(theme::gradient(mem_pct / 100.0))),
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
