//! The Claude sessions card. Sourced from processes, so it lists every running
//! session — including the ones holding no listening socket, which a
//! port-sourced list cannot see.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::{App, Focus};
use crate::sampler::sessions::ClaudeSession;

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
    let cursor = focused.then(|| app.selected());
    let offset = super::scroll::offset(visible, cursor);

    // tty and CPU are fixed-width; the project takes what is left. Reserve them
    // first so a long project name cannot push them off the edge.
    const TTY_W: usize = 8;
    const CPU_W: usize = 6;

    // The tty is only ever an answer to "which of these two identical rows is
    // which". On a project with a single session it is 8 columns telling you
    // something you cannot act on — you cannot map `ttys004` back to a window
    // without running `tty` in every terminal you have open.
    //
    // So it appears per row, and the column is reserved for the whole card
    // only when some row needs it: reserving per row instead would let the
    // CPU figures jump left and right down the list.
    let collides = |s: &ClaudeSession| {
        sessions.iter().filter(|o| o.project == s.project).count() > 1
    };
    let any_collision = sessions.iter().any(collides);
    let tty_w = if any_collision { TTY_W } else { 0 };
    let label_w = (inner.width as usize).saturating_sub(tty_w + CPU_W + 2);

    let lines: Vec<Line> = sessions
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(i, s)| {
            let selected = cursor == Some(i);
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
                Span::styled(format!(" {:<w$}", super::text::trunc(&s.project, label_w), w = label_w), base),
                Span::styled(
                    // Blank, not an em-dash, on a row that doesn't collide:
                    // a dash would read as "unknown tty" when the truth is
                    // "you don't need one".
                    match collides(s) {
                        true => format!("{:<w$}", s.tty.as_deref().unwrap_or("—"), w = tty_w),
                        false => " ".repeat(tty_w),
                    },
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
    use super::ClaudeSession;

    fn draw(w: u16, h: u16) -> Vec<String> {
        draw_app(&App::demo(), w, h)
    }

    fn draw_app(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::render(f, f.area(), app)).unwrap();
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
    fn a_project_with_only_one_session_shows_no_tty() {
        // The tty exists to tell two rows apart. On a row that is already
        // unique it is 8 columns of noise you cannot act on — you cannot map
        // ttys004 back to a window without running `tty` in each terminal.
        let out = draw(40, 8).join("\n");
        assert!(
            !out.contains("ttys004"),
            "whirr has one session, so its tty should not be shown:\n{out}"
        );
    }

    #[test]
    fn with_no_collisions_at_all_the_project_name_takes_the_whole_row() {
        // Nothing to disambiguate anywhere, so the column itself goes and the
        // name gets the width back. The name below is 26 characters: it fits
        // the 30 columns available once the tty is gone, and would not have
        // fit the 22 it left behind.
        let mut app = App::demo();
        let slow = app.slow.as_mut().expect("demo() ingests a slow snapshot");
        slow.sessions = vec![
            ClaudeSession {
                pid: 1,
                project: "a-project-with-a-long-name".into(),
                tty: Some("ttys001".into()),
            },
            ClaudeSession { pid: 2, project: "other".into(), tty: Some("ttys002".into()) },
        ];
        let out = draw_app(&app, 40, 8).join("\n");
        assert!(!out.contains("ttys001"), "no collisions, so no tty column:\n{out}");
        assert!(
            out.contains("a-project-with-a-long-name"),
            "the reclaimed columns should go to the name:\n{out}"
        );
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
