//! The Claude sessions card. Sourced from processes, so it lists every running
//! session — including the ones holding no listening socket, which a
//! port-sourced list cannot see.

use std::time::Duration;

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;


use crate::app::{App, Focus};
use crate::sampler::claude_state::{self, Activity, SessionState};
use super::theme::Theme;
use crate::sampler::sessions::ClaudeSession;

// `glyph`, `tone` and `label` are the shared vocabulary for a session's
// state, not private to this card: the grouped ports card wears them one tier
// down and the details dialog spells them out. They live here because this is
// the card that defines the look — but there is exactly one of each, because
// three renderers answering "what colour is this session" differently is how
// one design language becomes two.

/// The shape of a session at a glance, before any word is read: filled is
/// working, half-filled is running with nobody watching, hollow is waiting for
/// you. Three shapes rather than one per state — the word beside it says
/// whether "running without you" is a loop or a shell, and a row too narrow
/// for the word is still readable as the thing that matters.
pub fn glyph(a: &Activity) -> &'static str {
    match a {
        Activity::Busy => "●",
        Activity::Loop { .. } | Activity::BgJob | Activity::Scheduled { .. } => "◐",
        Activity::Idle { .. } => "○",
        Activity::Unknown => "·",
    }
}

/// The colour a session's state wears. Amber is the whole anomaly treatment —
/// a loop, an orphaned shell, a turn that never ended, a session open for days
/// — so the state word says which and no marker column has to repeat it.
pub fn tone(st: &SessionState, t: &Theme) -> Color {
    if st.warn {
        t.amber
    } else if matches!(st.activity, Activity::Busy) {
        t.accent
    } else {
        t.dim
    }
}

/// A duration in one unit, the largest that leaves a number worth reading.
/// The card has eight columns for a whole state; "4m" earns its place there
/// and "4m 12s" does not.
fn brief(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=172_799 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

/// What the state column says. Empty when there is nothing to say, so a
/// session whose state whirr cannot read shows blank rather than a guess.
pub fn label(st: &SessionState) -> String {
    match &st.activity {
        // Subagents only ever run inside a turn, so they qualify "busy"
        // rather than standing as a state of their own.
        Activity::Busy if st.subagents > 0 => format!("busy \u{00d7}{}", st.subagents),
        Activity::Busy => "busy".into(),
        Activity::Loop { wakes_in } => format!("loop {}", brief(*wakes_in)),
        Activity::BgJob => "bg job".into(),
        // No countdown, because nothing on disk says when the next one is.
        // The word alone is the warning: this session wakes without you.
        Activity::Scheduled { .. } => "scheduled".into(),
        // How long only matters once it is long enough to be a surprise;
        // until then it is a number that changes every second and says
        // nothing.
        // How long only matters once it is long enough to be a surprise;
        // until then it is a number that changes every second and says
        // nothing.
        Activity::Idle { since: Some(d) } if st.warn => format!("idle {}", brief(*d)),
        Activity::Idle { .. } => "idle".into(),
        Activity::Unknown => String::new(),
    }
}

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let focused = matches!(app.focus, Focus::Sessions);
    let block = app.theme.panel_block("claude sessions", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let sessions = app.sessions();
    // Derived here, once per frame, rather than stored beside the facts: a
    // countdown that was computed when the sampler last ran would only move
    // every ten seconds.
    let now = std::time::SystemTime::now();
    if sessions.is_empty() {
        f.render_widget(
            Paragraph::new("none running").style(Style::default().fg(app.theme.dim)),
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
    // "scheduled" is the longest thing the column ever says, one wider than
    // "loop 40s" and "idle 14d".
    const STATE_W: usize = 9;
    // The glyph and the space after it. Unlike the words, this is never
    // dropped: it is two columns, and it is the whole point of the card.
    const GLYPH_W: usize = 2;
    // Below this a name is being cut to make room for a word that repeats
    // what the glyph already said, which is a bad trade at any width.
    const MIN_LABEL: usize = 12;

    // The tty is only ever an answer to "which of these two identical rows is
    // which". On a project with a single session it is 8 columns telling you
    // something you cannot act on — you cannot map `ttys004` back to a window
    // without running `tty` in every terminal you have open.
    //
    // So it appears per row, and the column is reserved for the whole card
    // only when some row needs it: reserving per row instead would let the
    // CPU figures jump left and right down the list.
    // A host title already identifies the session, so a tty next to it would
    // be answering a question nobody has. Collision is judged on what the row
    // actually shows, not on the project underneath it.
    // The project stays even when the host supplies a title. A title says
    // what the session is *doing*; the project says which codebase it is
    // doing it in, and two sessions can easily be doing similar things in
    // different repos.
    fn shown(s: &ClaudeSession) -> &str {
        s.title.as_deref().unwrap_or(&s.project)
    }
    let collides = |s: &ClaudeSession| {
        s.title.is_none() && sessions.iter().filter(|o| shown(o) == shown(s)).count() > 1
    };
    let any_collision = sessions.iter().any(collides);
    let tty_w = if any_collision { TTY_W } else { 0 };
    let fixed = GLYPH_W + tty_w + CPU_W + 2;
    // The words go only where they fit. A narrow card keeps the glyph and
    // gives every remaining column to the name, which is the same design one
    // size down rather than a different one.
    let room = (inner.width as usize).saturating_sub(fixed);
    // Reserved for the whole card only when some row has something to put
    // there, the same rule the tty column follows: eight blank columns on
    // every row would be worse than the glyph carrying it alone.
    let any_state =
        sessions.iter().any(|s| !label(&claude_state::state(&s.facts, now)).is_empty());
    let state_w = if any_state && room >= MIN_LABEL + STATE_W { STATE_W } else { 0 };
    let label_w = room - state_w;
    // The project gets what it needs up to a ceiling; the title takes the
    // rest, and gets nothing when there is nothing to spare.
    let widest_project = sessions.iter().map(|s| s.project.chars().count()).max().unwrap_or(0);
    let any_title = sessions.iter().any(|s| s.title.is_some());
    let proj_w = if any_title { widest_project.min(label_w.saturating_sub(8)).max(1) } else { label_w };
    let title_w = label_w.saturating_sub(proj_w);

    let lines: Vec<Line> = sessions
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(i, s)| {
            let selected = cursor == Some(i);
            let base = if selected {
                Style::default().fg(app.theme.bg_cell).bg(app.theme.accent)
            } else {
                Style::default().fg(app.theme.text)
            };
            let cpu = match app.cpu_of(s.pid) {
                Some(c) => format!("{c:>5.1}%"),
                // An em-dash with no percent sign: unknown, not idle.
                None => format!("{:>w$}", "—", w = CPU_W),
            };
            let st = claude_state::state(&s.facts, now);
            let tone = tone(&st, &app.theme);
            Line::from(vec![
                Span::styled(
                    format!(" {}", glyph(&st.activity)),
                    if selected { base } else { Style::default().fg(tone) },
                ),
                // The host's own title when it has one — cmux names a
                // workspace after the task the session is doing, which beats
                // a project directory. Display only: the sort stays on
                // project/tty/pid, because a title changes every few seconds
                // and rows must not reorder under the cursor.
                // Project first — it is what you scan for — then the host's
                // own title in the space that is left. The title is truncated
                // rather than the project: losing the end of "…pull latest
                // changes" costs less than losing which repo it is.
                Span::styled(format!(" {:<w$}", super::text::trunc(&s.project, proj_w), w = proj_w), base),
                Span::styled(
                    match &s.title {
                        Some(t) if title_w > 1 => {
                            format!(" {:<w$}", super::text::trunc(t, title_w - 1), w = title_w - 1)
                        }
                        _ => " ".repeat(title_w),
                    },
                    if selected { base } else { Style::default().fg(app.theme.dim) },
                ),
                Span::styled(
                    // Blank, not an em-dash, on a row that doesn't collide:
                    // a dash would read as "unknown tty" when the truth is
                    // "you don't need one".
                    match collides(s) {
                        true => format!("{:<w$}", s.tty.as_deref().unwrap_or("—"), w = tty_w),
                        false => " ".repeat(tty_w),
                    },
                    if selected { base } else { Style::default().fg(app.theme.dim) },
                ),
                Span::styled(
                    match state_w {
                        0 => String::new(),
                        w => format!("{:<w$}", super::text::trunc(&label(&st), w), w = w),
                    },
                    if selected { base } else { Style::default().fg(tone) },
                ),
                Span::styled(
                    cpu,
                    if selected { base } else { Style::default().fg(app.theme.accent) },
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

    use std::time::Duration;

    use crate::app::App;
    use super::{glyph, label, tone, Activity, ClaudeSession, SessionState};

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
                title: None,
                jumpable: false,
                tty: Some("ttys001".into()),
                facts: Default::default(),
                record: None,
            },
            ClaudeSession { pid: 2, project: "other".into(), title: None, jumpable: false, tty: Some("ttys002".into()), facts: Default::default(), record: None },
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
    fn every_state_says_what_it_is() {
        let out = draw(160, 8).join("\n");
        // One demo session per state, so this covers the whole vocabulary.
        for word in ["busy ×2", "loop 4m", "idle", "bg job", "scheduled"] {
            assert!(out.contains(word), "state {word:?} missing:\n{out}");
        }
        for shape in ['●', '◐', '○'] {
            assert!(out.contains(shape), "glyph {shape:?} missing:\n{out}");
        }
    }

    #[test]
    fn a_scheduled_session_says_so_instead_of_saying_idle() {
        // A cron or a `/schedule` wakes the session and spends tokens with
        // nobody watching. Nothing on disk says when the next one lands, so
        // there is no countdown to show and none is invented.
        let st = SessionState {
            activity: Activity::Scheduled { last_fire: Some(Duration::from_secs(1800)) },
            warn: true,
            ..SessionState::default()
        };
        assert_eq!(label(&st), "scheduled");
        assert_eq!(glyph(&st.activity), glyph(&Activity::BgJob), "same shape as the others");
        assert_eq!(tone(&st, &crate::ui::theme::Theme::dark()), crate::ui::theme::Theme::dark().amber);
    }

    #[test]
    fn a_countdown_says_when_the_loop_starts_again() {
        // The point of showing a loop at all: not just that one is armed, but
        // how long you have before it spends anything.
        assert_eq!(
            label(&SessionState {
                activity: Activity::Loop { wakes_in: Duration::from_secs(260) },
                subagents: 0,
                warn: true
            }),
            "loop 4m"
        );
        assert_eq!(
            label(&SessionState {
                activity: Activity::Loop { wakes_in: Duration::from_secs(40) },
                subagents: 0,
                warn: true
            }),
            "loop 40s"
        );
    }

    #[test]
    fn idle_says_how_long_only_once_that_is_a_surprise() {
        // A number that changes every second and means nothing is noise; the
        // same number after a fortnight is the whole message.
        let fresh = SessionState {
            activity: Activity::Idle { since: Some(Duration::from_secs(90)) },
            subagents: 0,
            warn: false,
        };
        assert_eq!(label(&fresh), "idle");
        let forgotten = SessionState { warn: true, ..fresh };
        assert_eq!(label(&forgotten), "idle 1m");
    }

    #[test]
    fn a_session_whose_state_cannot_be_read_says_nothing_rather_than_guessing() {
        assert_eq!(label(&SessionState::default()), "");
        // And with nothing to put there, the column is not reserved at all.
        let mut app = App::demo();
        let slow = app.slow.as_mut().expect("demo() ingests a slow snapshot");
        for s in slow.sessions.iter_mut() {
            s.facts = Default::default();
        }
        let out = draw_app(&app, 160, 8).join("\n");
        for word in ["busy", "loop", "idle", "bg job"] {
            assert!(!out.contains(word), "nothing known, so nothing said: {word:?}\n{out}");
        }
    }

    #[test]
    fn a_narrow_card_keeps_the_shape_and_drops_the_word() {
        // The same design one size down, not a different one: the glyph is
        // two cells and never goes, the words go when a name would have to be
        // cut to seat them.
        let out = draw(36, 8).join("\n");
        assert!(out.contains('◐'), "the shape must survive any width:\n{out}");
        assert!(!out.contains("bg job"), "the word should have given up its columns:\n{out}");
    }

    #[test]
    fn one_rule_decides_what_colour_a_session_is() {
        // There used to be three copies of this and they disagreed: the card
        // dimmed an idle session, the grouped card did not, and the dialog had
        // no busy branch at all.
        let th = crate::ui::theme::Theme::dark();
        let idle = SessionState {
            activity: Activity::Idle { since: None },
            ..SessionState::default()
        };
        let busy = SessionState { activity: Activity::Busy, ..SessionState::default() };
        let looping = SessionState {
            activity: Activity::Loop { wakes_in: Duration::from_secs(60) },
            warn: true,
            ..SessionState::default()
        };
        assert_eq!(tone(&idle, &th), th.dim, "waiting for you is quiet");
        assert_eq!(tone(&busy, &th), th.accent, "working is the accent");
        assert_eq!(tone(&looping, &th), th.amber, "running without you is amber");
        // Warn wins over everything, including busy.
        let stalled = SessionState { activity: Activity::Busy, warn: true, ..SessionState::default() };
        assert_eq!(tone(&stalled, &th), th.amber);
    }

    #[test]
    fn a_session_running_without_you_is_the_one_that_stands_out() {
        // Amber is the whole anomaly treatment. A loop and an orphaned shell
        // wear it; a session working while you watch does not.
        let app = App::demo();
        let amber = app.theme.amber;
        let mut t = Terminal::new(TestBackend::new(160, 8)).unwrap();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let b = t.backend().buffer().clone();
        let tone = |needle: &str| {
            (0..8).find_map(|y| {
                let row: String =
                    (0..160).map(|x| b[(x, y)].symbol().to_string()).collect::<String>();
                row.contains(needle).then(|| {
                    let at = row.find(needle).expect("just matched");
                    b[(at as u16, y)].style().fg
                })
            })
            .flatten()
        };
        assert_eq!(tone("loop 4m"), Some(amber), "an armed loop must stand out");
        assert_eq!(tone("bg job"), Some(amber), "an orphaned shell must stand out");
        assert_ne!(tone("busy"), Some(amber), "working while you watch is normal");
        assert_ne!(tone("idle"), Some(amber), "so is waiting for you");
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
