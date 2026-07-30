//! The Claude sessions card. Sourced from processes, so it lists every running
//! session — including the ones holding no listening socket, which a
//! port-sourced list cannot see.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::{App, Focus};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let focused = matches!(app.focus, Focus::Sessions);
    let block = theme::panel_block("claude sessions", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let sessions = app.sessions();
    if sessions.is_empty() {
        f.render_widget(
            Paragraph::new("none running").style(Style::default().fg(theme::DIM)),
            inner,
        );
        return;
    }

    let visible = inner.height as usize;
    let offset = if focused { app.selected.saturating_sub(visible.saturating_sub(1)) } else { 0 };

    // tty and CPU are fixed-width; the project takes what is left. Reserve them
    // first so a long project name cannot push them off the edge.
    const TTY_W: usize = 8;
    const CPU_W: usize = 6;
    let label_w = (inner.width as usize).saturating_sub(TTY_W + CPU_W + 2);

    let lines: Vec<Line> = sessions
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(i, s)| {
            let selected = focused && i == app.selected;
            let base = if selected {
                Style::default().fg(theme::BG_CELL).bg(theme::ACCENT)
            } else {
                Style::default().fg(theme::TEXT)
            };
            let cpu = match app.cpu_of(s.pid) {
                Some(c) => format!("{c:>5.1}%"),
                // An em-dash with no percent sign: unknown, not idle.
                None => format!("{:>w$}", "—", w = CPU_W),
            };
            Line::from(vec![
                Span::styled(format!(" {:<w$}", super::ports::trunc(&s.project, label_w), w = label_w), base),
                Span::styled(
                    format!("{:<w$}", s.tty.as_deref().unwrap_or("—"), w = TTY_W),
                    if selected { base } else { Style::default().fg(theme::DIM) },
                ),
                Span::styled(
                    cpu,
                    if selected { base } else { Style::default().fg(theme::ACCENT) },
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let b = t.backend().buffer().clone();
        (0..h).map(|y| (0..w).map(|x| b[(x, y)].symbol().to_string()).collect()).collect()
    }

    #[test]
    fn two_sessions_in_one_project_are_told_apart_by_tty() {
        let out = draw(40, 8).join("\n");
        // This is the bug that motivated the card: both rows say "axterio",
        // so the tty is the only thing distinguishing them.
        assert!(out.contains("ttys020"), "first axterio session's tty missing:\n{out}");
        assert!(out.contains("ttys021"), "second axterio session's tty missing:\n{out}");
    }

    #[test]
    fn a_session_shows_cpu_when_known_and_a_dash_when_not() {
        let out = draw(40, 8).join("\n");
        assert!(out.contains("8.1"), "known CPU missing:\n{out}");
        assert!(out.contains('—'), "unknown CPU should render an em-dash:\n{out}");
        assert!(!out.contains("—%"), "em-dash must not be followed by a percent sign");
    }

    #[test]
    fn sessions_have_no_port_column() {
        let out = draw(40, 8).join("\n");
        assert!(!out.contains(':'), "session rows must not show ports:\n{out}");
    }

    #[test]
    fn nothing_truncates_mid_value_at_forty_columns() {
        for line in draw(40, 8) {
            let t = line.trim_end();
            assert!(!t.ends_with("tty"), "tty name cut short: {t:?}");
            assert!(!t.ends_with("ttys"), "tty name cut short: {t:?}");
        }
    }
}
