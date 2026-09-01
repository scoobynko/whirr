//! The session details dialog, raised with `d` from the sessions card.
//!
//! The card has one row per session and a state column eight cells wide. This
//! is where the same session gets to answer the question the card only raises:
//! not *that* something is running, but what — which subagent, on which task,
//! and what command a background shell is actually running.
//!
//! Built as lines rather than drawn: the dialog chrome is `modal::render`,
//! which the kill confirmation and the settings dialog already share, and the
//! only thing worth testing here is what it says.

use std::time::SystemTime;

use ratatui::prelude::*;

use super::theme::Theme;
use crate::sampler::claude_state::{self, Activity, ActivityFacts, SessionState};
use crate::sampler::sessions::ClaudeSession;
use crate::units::{fmt_bytes, fmt_duration};

/// Width of the label gutter. `subagents` is the longest label, and the
/// continuation lines under it hang at the same indent.
const LABEL_W: usize = 10;

/// The dialog's title: what the row itself is called.
pub fn title(s: &ClaudeSession) -> &str {
    s.title.as_deref().unwrap_or(&s.project)
}

/// The state line, spelled out. The card had eight columns and had to say
/// `loop 4m`; here there is room to say what that means.
fn state_line(st: &SessionState, facts: &ActivityFacts, now: SystemTime) -> String {
    match &st.activity {
        Activity::Busy => match facts.last_write.and_then(|t| now.duration_since(t).ok()) {
            // The heartbeat that separates a long turn from a hung one, and
            // the reason the card did or did not flag this row.
            Some(d) => format!("busy · writing {} ago", fmt_duration(d.as_secs())),
            None => "busy".into(),
        },
        Activity::Loop { wakes_in } => {
            format!("loop · wakes in {}", fmt_duration(wakes_in.as_secs()))
        }
        Activity::BgJob => "bg job · waiting on a shell".into(),
        Activity::Scheduled { last_fire } => {
            format!("scheduled · last ran {} ago", fmt_duration(last_fire.as_secs()))
        }
        Activity::Idle { since } => match since {
            Some(d) => format!("idle · {}", fmt_duration(d.as_secs())),
            None => "idle".into(),
        },
        Activity::Unknown => "state unknown".into(),
    }
}

/// One labelled row. `label` is empty on the continuation rows of a block, so
/// a list of subagents hangs under a single gutter entry rather than repeating
/// it down the side.
///
/// Values are truncated to what the dialog can actually show. The box stops
/// widening at `modal::MAX_COLS` and the paragraph does not wrap, so anything
/// longer would be severed mid-word at the border with nothing to say it had
/// been — and a background command or a worktree path is easily longer.
fn row(label: &str, value: &str, t: &Theme, style: Style) -> Line<'static> {
    let room = super::modal::MAX_COLS as usize - LABEL_W;
    Line::from(vec![
        Span::styled(format!("{label:<LABEL_W$}"), Style::default().fg(t.dim)),
        Span::styled(super::text::trunc(value, room), style),
    ])
}

/// Everything the dialog says about `s`.
///
/// `cpu` and `mem` come from the fast snapshot, joined by pid the way the card
/// joins them; either can be missing for a process that arrived between
/// samples, and a missing one drops its clause rather than printing a zero.
pub fn lines(
    s: &ClaudeSession,
    t: &Theme,
    cpu: Option<f32>,
    mem: Option<u64>,
    now: SystemTime,
) -> Vec<Line<'static>> {
    let st = claude_state::state(&s.facts, now);
    let text = Style::default().fg(t.text);
    let dim = Style::default().fg(t.dim);
    let mut out: Vec<Line> = Vec::new();

    let tone = super::sessions::tone(&st, t);
    out.push(Line::from(vec![
        Span::styled(format!("{} ", super::sessions::glyph(&st.activity)), Style::default().fg(tone)),
        Span::styled(state_line(&st, &s.facts, now), Style::default().fg(tone)),
    ]));

    // Each block is skipped entirely when it has nothing to say, so an idle
    // session with no subagents and no shell is a short dialog rather than a
    // tall one full of dashes.
    if !s.facts.subagents.is_empty() {
        out.push(Line::from(""));
        for (i, a) in s.facts.subagents.iter().enumerate() {
            let kind = match &a.model {
                Some(m) => format!("{} · {m}", a.kind),
                None => a.kind.clone(),
            };
            out.push(row(if i == 0 { "subagents" } else { "" }, &kind, t, text));
            if !a.task.is_empty() {
                // The task is the point of the whole dialog, so it is the one
                // thing here that is not dimmed.
                out.push(row("", &a.task, t, Style::default().fg(t.accent)));
            }
        }
    }

    if !s.facts.shells.is_empty() {
        out.push(Line::from(""));
        for (i, cmd) in s.facts.shells.iter().enumerate() {
            out.push(row(if i == 0 { "shell" } else { "" }, cmd, t, text));
        }
    }

    out.push(Line::from(""));
    if let Some(rec) = &s.record {
        out.push(row("project", &rec.cwd.to_string_lossy(), t, text));
        if let Some(account) = rec.account() {
            out.push(row("account", account, t, text));
        }
    }

    // Identity and cost on two lines rather than four labelled rows: none of
    // it is worth a gutter of its own, and together each reads as one fact
    // about the process.
    let mut ident = vec![format!("pid {}", s.pid)];
    ident.extend(s.tty.clone());
    ident.extend(s.record.as_ref().and_then(|r| r.version.clone()).map(|v| format!("claude {v}")));
    out.push(Line::styled(ident.join(" · "), dim));

    let mut usage: Vec<String> = Vec::new();
    let started = s.record.as_ref().and_then(|r| r.started_at);
    if let Some(open) = started.and_then(|t| now.duration_since(t).ok()) {
        usage.push(format!("open {}", fmt_duration(open.as_secs())));
    }
    usage.extend(cpu.map(|c| format!("cpu {c:.1}%")));
    usage.extend(mem.map(|m| format!("mem {}", fmt_bytes(m))));
    if !usage.is_empty() {
        out.push(Line::styled(usage.join(" · "), dim));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::sampler::claude_state::Subagent;

    /// The dialog's text, one string per line.
    fn text(s: &ClaudeSession) -> Vec<String> {
        lines(s, &Theme::dark(), Some(8.1), Some(300_000), SystemTime::now())
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>().trim_end().to_string())
            .collect()
    }

    fn demo(i: usize) -> ClaudeSession {
        App::demo().sessions()[i].clone()
    }

    #[test]
    fn a_busy_session_names_the_subagents_and_their_tasks() {
        // The whole reason the dialog exists: "2 subagents" tells you nothing
        // you can act on, and the task line tells you everything.
        let t = text(&demo(0)).join("\n");
        assert!(t.contains("general-purpose · haiku"), "{t}");
        assert!(t.contains("Run full quality checks"), "{t}");
        assert!(t.contains("Explore · sonnet"), "{t}");
        assert!(t.contains("Find every call site of useChart"), "{t}");
        // The label appears once, not beside every agent.
        assert_eq!(t.matches("subagents").count(), 1, "{t}");
    }

    #[test]
    fn a_background_job_names_the_command() {
        let t = text(&demo(3)).join("\n");
        assert!(t.contains("bg job"), "{t}");
        assert!(t.contains("CI=true pnpm test"), "the command is the point: {t}");
    }

    #[test]
    fn the_state_line_spells_out_what_the_card_abbreviated() {
        assert!(text(&demo(0))[0].contains("busy · writing 3s ago"));
        assert!(text(&demo(1))[0].contains("loop · wakes in 4m"));
        assert!(text(&demo(2))[0].contains("idle · 1m"));
        assert!(text(&demo(3))[0].contains("bg job"));
        // The card has room for "scheduled" and nothing else; here there is
        // room to say when it last happened, which is the only fact there is.
        assert!(text(&demo(4))[0].contains("scheduled · last ran 30m ago"));
    }

    #[test]
    fn a_session_with_nothing_running_gets_a_short_dialog() {
        // Blocks are skipped rather than filled with dashes, so a quiet
        // session is not a tall box of nothing.
        let quiet = demo(2);
        let t = text(&quiet).join("\n");
        assert!(!t.contains("subagents"), "no subagents block: {t}");
        assert!(!t.contains("shell"), "no shell block: {t}");
        assert!(t.contains("idle"), "but it still says what it is doing: {t}");
    }

    #[test]
    fn the_plumbing_identifies_the_session() {
        let t = text(&demo(0)).join("\n");
        assert!(t.contains("/Users/me/Projects/axterio"), "full path, not the basename: {t}");
        assert!(t.contains(".claude"), "which account: {t}");
        assert!(t.contains("pid 601"), "{t}");
        assert!(t.contains("ttys020"), "{t}");
        assert!(t.contains("claude 2.1.252"), "{t}");
        assert!(t.contains("cpu 8.1%"), "{t}");
        assert!(t.contains("open "), "{t}");
    }

    #[test]
    fn a_session_whirr_knows_nothing_about_still_gets_a_dialog() {
        // Every optional clause absent at once: no state file, no tty, no
        // fast-snapshot row. It must render rather than panic or lie.
        let mut bare = demo(0);
        bare.facts = Default::default();
        bare.record = None;
        bare.tty = None;
        let out = lines(&bare, &Theme::dark(), None, None, SystemTime::now());
        let t: String = out
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(t.contains("state unknown"), "no invented state: {t}");
        assert!(t.contains("pid 601"), "the pid is always knowable: {t}");
        assert!(!t.contains("cpu"), "a missing sample drops its clause: {t}");
    }

    #[test]
    fn a_model_less_subagent_shows_only_its_kind() {
        let mut s = demo(0);
        s.facts.subagents =
            vec![Subagent { kind: "Explore".into(), model: None, task: "Look around".into() }];
        let t = text(&s).join("\n");
        assert!(t.contains("Explore"), "{t}");
        assert!(!t.contains("Explore ·"), "no dangling separator: {t}");
    }

    #[test]
    fn a_long_command_is_truncated_rather_than_severed_at_the_border() {
        // The box stops widening at modal::MAX_COLS and the paragraph does not
        // wrap, so an untruncated value would be cut mid-word with nothing to
        // say it had been. A background command is easily this long.
        let mut s = demo(3);
        s.facts.shells = vec!["CI=true pnpm test --reporter verbose ".repeat(6)];
        let line = text(&s).into_iter().find(|l| l.contains("CI=true")).expect("shell row");
        assert!(line.chars().count() <= super::super::modal::MAX_COLS as usize, "{line:?}");
        assert!(line.ends_with('…'), "and it should say it was cut: {line:?}");
    }

    #[test]
    fn the_title_prefers_the_hosts_own_name_for_the_session() {
        assert_eq!(title(&demo(2)), "✳ Fix the port picker");
        assert_eq!(title(&demo(0)), "axterio", "falling back to the project");
    }
}
