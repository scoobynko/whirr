//! What each Claude Code session is doing right now.
//!
//! The sessions card already knows a session exists. This says whether it is
//! working, waiting for you, or about to start again on its own — which is the
//! difference between a session you are using and one that is running
//! unattended. Three sources, none of them expensive:
//!
//! * `<root>/sessions/<pid>.json` — Claude Code writes its own busy/idle flag
//!   there on every transition, so nothing has to be inferred from CPU.
//! * the process tree — a shell the session started is still a live child of
//!   it, so a background job that outlived its turn shows up for free.
//! * the transcript tail — an armed wakeup is held in memory and never
//!   written down, but the `ScheduleWakeup` call that armed it is in the log
//!   with its delay, which is enough to rebuild the countdown.
//!
//! `roots` is plural on purpose. `CLAUDE_CONFIG_DIR` moves this whole tree,
//! and a work account running beside a personal one is common enough that
//! reading only `~/.claude` would quietly show half a machine.
//!
//! Pure by design, the same way `sessions` is: the reads live in `slow.rs`
//! and arrive here as `ActivityFacts`. Everything below is a parser, a policy
//! or a type — nothing here touches the filesystem, so all of it is testable
//! against a string.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A session flagged busy whose transcript has not been touched this long has
/// stopped working without saying so.
///
/// Deliberately not measured from `statusUpdatedAt`: that is written once when
/// the turn begins and not again until it ends, so any threshold on it flags a
/// long agentic run as stuck. A session that is genuinely working writes to
/// its transcript every few seconds; one that hung froze at the moment it did.
///
/// The one thing that writes nothing while working is a long shell call, which
/// is why `state` also requires that no shell is running before it says a word.
const STALLED: Duration = Duration::from_secs(15 * 60);

/// Idle this long is a session you have forgotten about. It costs nothing in
/// itself, but it is holding its MCP servers and language servers open.
const FORGOTTEN: Duration = Duration::from_secs(24 * 60 * 60);

/// A subagent transcript touched this recently counts as one still running.
///
/// A heuristic, and the honest name for it. Three signals were tried and none
/// of them is exact: an async dispatch is acknowledged the instant it is
/// launched, so an outstanding call means "started", not "running"; the
/// completion notice is not written in a shape that always parses; and an
/// agent that is thinking writes nothing at all — measured here, 106 seconds
/// of legitimate silence from an agent that was working the whole time.
///
/// So mtime it is, with a window sized to that measurement rather than
/// guessed. The cost is the other direction: an agent that has finished stays
/// on the count until its log goes cold. Better a number that lags than one
/// that drops a session's work while it is still being done.
pub const SUBAGENT_HOT: Duration = Duration::from_secs(5 * 60);

/// How far past an armed wakeup's own timestamp the transcript may have grown
/// before the wakeup is treated as spent rather than pending.
///
/// A session with a wakeup armed is by definition quiet, so its transcript
/// stops growing the moment the call is logged. Growth well past that means
/// the loop already fired, or was interrupted and never rearmed.
const WAKE_SETTLE: Duration = Duration::from_secs(120);

/// Claude Code's own account of itself, from `sessions/<pid>.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Busy,
    Idle,
}

/// What a session is doing, in the order that matters when more than one is
/// true at once.
#[derive(Clone, Debug, PartialEq)]
pub enum Activity {
    /// Working on a turn right now.
    Busy,
    /// Quiet, but it has armed its own wakeup: it will start again with
    /// nobody at the keyboard.
    Loop { wakes_in: Duration },
    /// Quiet, but a schedule can start it: a live cron, or a scheduled task
    /// that has woken it before. Sibling of `Loop` without the countdown,
    /// because the schedule itself lives on the server and the next fire time
    /// appears nowhere on disk.
    Scheduled { last_fire: Option<Duration> },
    /// Quiet, but a shell it started is still running.
    BgJob,
    /// Waiting for a human. `since` is how long, when that is knowable.
    Idle { since: Option<Duration> },
    /// No session file and nothing else to go on — an older Claude Code, or a
    /// config root whirr cannot read.
    Unknown,
}

/// What a sampler observed about a session, as moments rather than ages.
///
/// Timestamps, deliberately: an age computed when the sample was taken is
/// already ten seconds stale by the next one, which a countdown makes visible.
/// Everything derived from these is derived against the `now` of the frame
/// that asks, so nothing here ever goes off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActivityFacts {
    pub status: Option<Status>,
    /// When the session last entered that status.
    pub status_since: Option<SystemTime>,
    /// What each live shell child is running, once the wrapper is stripped
    /// off. One field rather than a count beside a command: those two could
    /// disagree, and "no shells but here is what one is running" is not a
    /// state anything should be able to represent.
    pub shells: Vec<String>,
    /// The subagents running right now.
    pub subagents: Vec<Subagent>,
    /// When an armed wakeup will fire, if one is armed.
    pub wake_at: Option<SystemTime>,
    /// When anything was last written to the session's transcript. This is the
    /// heartbeat of a session that is working.
    pub last_write: Option<SystemTime>,
    /// When a scheduled task last woke this session, if one ever has.
    pub last_scheduled_fire: Option<SystemTime>,
    /// Live crons, in Claude Code's own words: "Every 5 minutes". A cron
    /// outlives the turn that made it, so this is the signal that matters —
    /// a fire only proves one already happened.
    pub crons: Vec<String>,
}

/// What the facts add up to. Derived on demand and never stored: every field
/// here is a conclusion, so keeping a copy beside the evidence would only
/// create something that can drift from it.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionState {
    pub activity: Activity,
    /// How many subagents are out — a count, because that is all the card and
    /// the state word need. The dialog reads the facts for who they are.
    pub subagents: usize,
    /// Something here is running without you: a loop, an orphaned shell, a
    /// turn that never finished, or a session left open for days.
    pub warn: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState { activity: Activity::Unknown, subagents: 0, warn: false }
    }
}

/// Decide what a session is doing.
///
/// Order is the whole design. Busy wins outright: a shell running inside a
/// turn you are watching is not a background job, and a wakeup armed earlier
/// in the same turn has already been superseded by the work happening now.
/// Below that, a loop outranks a shell because it restarts the model, and a
/// shell only spends a machine.
pub fn state(f: &ActivityFacts, now: SystemTime) -> SessionState {
    let since = |t: Option<SystemTime>| t.and_then(|t| now.duration_since(t).ok());
    let status_age = since(f.status_since);
    let writing_age = since(f.last_write);
    // A wakeup whose moment has passed is not pending: the session is either
    // already working again, or the loop ended without saying so.
    let wakes_in = f.wake_at.and_then(|t| t.duration_since(now).ok());

    let activity = if f.status == Some(Status::Busy) {
        Activity::Busy
    } else if let Some(w) = wakes_in {
        Activity::Loop { wakes_in: w }
    } else if !f.shells.is_empty() {
        Activity::BgJob
    } else if !f.crons.is_empty() || f.last_scheduled_fire.is_some() {
        // Below a background job, which is happening now, and above idle,
        // which is what this would otherwise read as: a session that takes
        // scheduled work spends tokens with nobody watching, and saying
        // "idle" about it is the one answer that helps least.
        Activity::Scheduled { last_fire: since(f.last_scheduled_fire) }
    } else if f.status == Some(Status::Idle) {
        Activity::Idle { since: status_age }
    } else {
        // Nothing said it is idle; it may simply be unreadable.
        Activity::Unknown
    };
    let warn = match &activity {
        // Busy but silent: no transcript writes and no shell to be waiting on.
        // An unreadable transcript says nothing either way, and inventing a
        // stall from missing evidence would cry wolf on every session whirr
        // cannot follow.
        Activity::Busy => f.shells.is_empty() && writing_age.is_some_and(|d| d >= STALLED),
        Activity::Loop { .. } | Activity::BgJob | Activity::Scheduled { .. } => true,
        Activity::Idle { since } => since.is_some_and(|d| d >= FORGOTTEN),
        Activity::Unknown => false,
    };
    SessionState { activity, subagents: f.subagents.len(), warn }
}

/// One running session as `sessions/<pid>.json` describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRecord {
    pub pid: i32,
    pub session_id: String,
    pub cwd: PathBuf,
    pub status: Option<Status>,
    pub status_updated_at: Option<SystemTime>,
    /// The Claude Code build, and when the process started.
    pub version: Option<String>,
    pub started_at: Option<SystemTime>,
    /// The config root this record was found under, so the transcript can be
    /// looked up in the same tree.
    pub root: PathBuf,
}

impl SessionRecord {
    /// Which login this session belongs to: the config root's own name,
    /// `.claude` or whatever a second account's tree is called. Two sessions
    /// with the same project name can be two different accounts.
    pub fn account(&self) -> Option<&str> {
        self.root.file_name().and_then(|n| n.to_str())
    }
}

/// Parse one `sessions/<pid>.json`. `root` is carried through rather than
/// derived, because the file does not name the tree it lives in.
pub fn parse_session_record(json: &str, root: &Path) -> Option<SessionRecord> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let status = match v.get("status").and_then(|s| s.as_str()) {
        Some("busy") => Some(Status::Busy),
        Some("idle") => Some(Status::Idle),
        _ => None,
    };
    Some(SessionRecord {
        pid: i32::try_from(v.get("pid")?.as_i64()?).ok()?,
        session_id: v.get("sessionId")?.as_str()?.to_string(),
        cwd: PathBuf::from(v.get("cwd")?.as_str()?),
        status,
        status_updated_at: v
            .get("statusUpdatedAt")
            .and_then(|t| t.as_u64())
            .map(|ms| UNIX_EPOCH + Duration::from_millis(ms)),
        version: v.get("version").and_then(|s| s.as_str()).map(str::to_string),
        started_at: v
            .get("startedAt")
            .and_then(|t| t.as_u64())
            .map(|ms| UNIX_EPOCH + Duration::from_millis(ms)),
        root: root.to_path_buf(),
    })
}


/// The directory name Claude Code gives a project: its path with every
/// separator, dot and space flattened to a dash.
pub fn project_dir_name(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' || c == ' ' || c == '_' { '-' } else { c })
        .collect()
}


/// One subagent a session has out right now.
///
/// The default is what an unreadable `.meta.json` leaves: a subagent that is
/// certainly running, described only as far as the disk allowed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subagent {
    /// `general-purpose`, `Explore`, or whatever the agent was dispatched as.
    pub kind: String,
    /// The model it was given, when the dispatch named one.
    pub model: Option<String>,
    /// The task it was handed. This is the line that answers "what is this
    /// session actually doing" — the count alone never does.
    pub task: String,
}


/// One `agent-<id>.meta.json`.
pub fn parse_subagent(json: &str) -> Option<Subagent> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    Some(Subagent {
        kind: v.get("agentType")?.as_str()?.to_string(),
        model: v.get("model").and_then(|m| m.as_str()).map(str::to_string),
        task: v.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string(),
    })
}

/// The command a background shell is actually running, dug out of the wrapper
/// Claude Code's Bash tool builds around it.
///
/// The wrapper sources a shell snapshot, sets options, drops an alias, then
/// `eval`s the real command and records the working directory. Showing it
/// verbatim would fill the dialog with a kilobyte of boilerplate and hide the
/// one line worth reading.
///
/// Anchored on the trailing `&& pwd -P`, because the middle varies: a command
/// that reads stdin is not given `< /dev/null`, so anchoring on that redirect
/// silently fell back to the raw argv for every heredoc — which is how this
/// was found. The anchor is matched from the right, since the suffix is
/// appended last and a command containing the same text cannot cut it short.
///
/// Newlines collapse to spaces: a heredoc is one argv element containing
/// several lines, and the dialog draws one line per row.
///
/// An argv without this shape is handed back whole. An unrecognised wrapper is
/// still better read than dropped.
pub fn shell_command(argv: &str) -> String {
    const OPEN: &str = "&& eval '";
    const CLOSE: &str = "&& pwd -P";
    let cmd = match (argv.find(OPEN).map(|i| i + OPEN.len()), argv.rfind(CLOSE)) {
        (Some(start), Some(end)) if end > start => argv[start..end]
            .trim_end()
            .trim_end_matches("< /dev/null")
            .trim_end()
            .trim_end_matches('\'')
            .trim(),
        _ => argv.trim(),
    };
    cmd.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Seconds since the epoch for the one timestamp shape the transcript uses,
/// `2026-09-01T08:57:30.679Z`. Not a date library: anything else is `None`.
fn epoch_secs(iso: &str) -> Option<u64> {
    let num = |r: std::ops::Range<usize>| iso.get(r).and_then(|s| s.parse::<i64>().ok());
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from the civil calendar, shifting the year to start in March so
    // the leap day lands at the end of it and needs no special case.
    let y = y - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3_600 + mi * 60 + s).ok()
}

/// The moment an armed `ScheduleWakeup` will fire, read out of the tail of a
/// transcript.
///
/// `grew_until` is the transcript's own mtime. A session with a wakeup armed
/// is quiet by definition, so its log stops growing the moment the call is
/// written; a log that kept growing well past it means the loop already fired
/// or was interrupted, and the record on disk is spent rather than pending.
pub fn armed_wake_at(tail: &str, grew_until: SystemTime) -> Option<SystemTime> {
    let (at, delay) = tail
        .lines()
        .rev()
        .filter(|l| l.contains("ScheduleWakeup"))
        .find_map(wakeup_in_line)?;
    // `{"stop": true}` is how a loop ends, and it is logged the same way as
    // the calls that arm one.
    let at = UNIX_EPOCH + Duration::from_secs(at);
    let delay = delay?;
    if grew_until.duration_since(at).is_ok_and(|d| d > WAKE_SETTLE) {
        return None;
    }
    Some(at + Duration::from_secs(delay))
}

/// What one slice of a transcript says about work arranged to happen without
/// the user: crons made and unmade, and scheduled tasks that fired.
///
/// Deliberately incremental. These records are written when the user asks for
/// them and then never again, so they end up arbitrarily far back — a cron set
/// two hours ago sits behind a megabyte of since. A tail scan cannot see it,
/// which is exactly how this was found.
#[derive(Debug, Default, PartialEq)]
pub struct Unattended {
    /// Cron ids created here, each with the schedule in Claude Code's words.
    pub created: Vec<(String, String)>,
    /// Cron ids deleted here.
    pub deleted: Vec<String>,
    /// The latest scheduled fire in this slice.
    pub last_fire: Option<SystemTime>,
}

/// Read cron and scheduled-fire records out of `text`.
///
/// Each of the three is identified by something in the record itself rather
/// than by pairing a call with its result. Pairing needed the call and the
/// result to survive the same pre-filter, and a `CronDelete` result does not
/// mention the word "Cron" anywhere: every deletion was silently dropped and
/// deleted crons stayed on the card. Keys that only cron records carry are a
/// harder thing to get wrong.
///
/// `humanSchedule` is the key for a creation, since that is the only place the
/// new id is written. A deletion names its id in the call itself, so it needs
/// no result at all.
pub fn scan_unattended(text: &str) -> Unattended {
    let mut out = Unattended::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v.get("subtype").and_then(|s| s.as_str()) == Some("scheduled_task_fire")
            && v.get("type").and_then(|t| t.as_str()) == Some("system")
        {
            if let Some(t) = v.get("timestamp").and_then(|t| t.as_str()).and_then(epoch_secs) {
                out.last_fire = Some(UNIX_EPOCH + Duration::from_secs(t));
            }
        }
        if let Some(result) = v.get("toolUseResult") {
            if let (Some(id), Some(schedule)) = (
                result.get("id").and_then(|i| i.as_str()),
                result.get("humanSchedule").and_then(|h| h.as_str()),
            ) {
                out.created.push((id.to_string(), schedule.to_string()));
            }
        }
        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let blocks = v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array());
        for c in blocks.into_iter().flatten() {
            if c.get("type").and_then(|t| t.as_str()) != Some("tool_use")
                || c.get("name").and_then(|n| n.as_str()) != Some("CronDelete")
            {
                continue;
            }
            if let Some(id) = c.get("input").and_then(|i| i.get("id")).and_then(|i| i.as_str()) {
                out.deleted.push(id.to_string());
            }
        }
    }
    out
}

/// One transcript line, as `(logged at, delay in seconds)`. A `None` delay is
/// a wakeup that stops the loop rather than arming it.
///
/// Only an assistant turn's own `tool_use` counts. The name appears in plenty
/// of other places — a tool listing, a system reminder, a user quoting it —
/// and none of those arm anything.
fn wakeup_in_line(line: &str) -> Option<(u64, Option<u64>)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let at = epoch_secs(v.get("timestamp")?.as_str()?)?;
    v.get("message")?.get("content")?.as_array()?.iter().find_map(|c| {
        if c.get("type")?.as_str()? != "tool_use" || c.get("name")?.as_str()? != "ScheduleWakeup" {
            return None;
        }
        let input = c.get("input")?;
        if input.get("stop").and_then(|s| s.as_bool()) == Some(true) {
            return Some((at, None));
        }
        Some((at, Some(input.get("delaySeconds")?.as_f64()? as u64)))
    })
}

/// Which of `pid`'s children are shells.
///
/// This is what a background `Bash` call looks like from outside: the tool
/// runs `/bin/zsh -c …` as a child of the session and leaves it running. The
/// session's other children are MCP servers and language servers, which are
/// node, python or uv — never a shell — so the executable name alone
/// separates them, without paying for an argv read per child.
///
/// Pids rather than a count, so the caller can read one argv for the dialog
/// without this function having to do IO of its own. Lowest pid first, which
/// on a wrapping-free counter is oldest first.
pub fn shell_children(
    pid: i32,
    parents: &HashMap<i32, i32>,
    exec: impl Fn(i32) -> Option<PathBuf>,
) -> Vec<i32> {
    let mut pids: Vec<i32> = parents
        .iter()
        .filter(|(_, &ppid)| ppid == pid)
        .map(|(&child, _)| child)
        .filter(|&child| exec(child).as_deref().is_some_and(is_shell))
        .collect();
    pids.sort_unstable();
    pids
}

fn is_shell(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("zsh" | "bash" | "sh" | "dash" | "fish")
    )
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed `now`, so every fact below is written as "this long before the
    /// frame that reads it" and no test depends on the clock.
    const NOW: SystemTime = UNIX_EPOCH;

    fn ago(secs: u64) -> SystemTime {
        NOW - Duration::from_secs(secs)
    }

    fn ahead(secs: u64) -> SystemTime {
        NOW + Duration::from_secs(secs)
    }

    fn facts() -> ActivityFacts {
        ActivityFacts {
            status: Some(Status::Idle),
            status_since: Some(ago(60)),
            ..Default::default()
        }
    }

    /// The state these facts add up to at `NOW`.
    fn derived(f: &ActivityFacts) -> SessionState {
        state(f, NOW)
    }

    #[test]
    fn a_busy_session_reads_as_busy() {
        let f = ActivityFacts { status: Some(Status::Busy), ..facts() };
        assert_eq!(derived(&f).activity, Activity::Busy);
        assert!(!derived(&f).warn, "working is the normal case, not a warning");
    }

    #[test]
    fn a_busy_session_writing_nothing_has_stopped_working() {
        let f = ActivityFacts {
            status: Some(Status::Busy),
            last_write: Some(ago(STALLED.as_secs() + 1)),
            ..facts()
        };
        assert_eq!(derived(&f).activity, Activity::Busy);
        assert!(derived(&f).warn, "a busy session with a frozen transcript must be flagged");
    }

    #[test]
    fn a_long_turn_is_not_a_stalled_one() {
        // The bug this rule replaces: `statusUpdatedAt` is written once when a
        // turn begins and not again until it ends, so any threshold on it
        // flags an agentic run that is working perfectly.
        let f = ActivityFacts {
            status: Some(Status::Busy),
            status_since: Some(ago(4 * 60 * 60)),
            last_write: Some(ago(3)),
            ..facts()
        };
        assert!(!derived(&f).warn, "four hours of a turn that is still writing is fine");
    }

    #[test]
    fn a_session_waiting_on_a_long_shell_is_not_stalled() {
        // The one way to work while writing nothing: a test suite or a build
        // running for minutes. The shell is the proof it is still going.
        let f = ActivityFacts {
            status: Some(Status::Busy),
            shells: vec!["pnpm test".into()],
            last_write: Some(ago(STALLED.as_secs() * 4)),
            ..facts()
        };
        assert!(!derived(&f).warn, "a live shell is a session still doing something");
    }

    #[test]
    fn a_busy_session_whose_transcript_cannot_be_read_is_not_accused() {
        let f = ActivityFacts { status: Some(Status::Busy), last_write: None, ..facts() };
        assert!(!derived(&f).warn, "missing evidence is not evidence of a stall");
    }

    #[test]
    fn an_armed_wakeup_reads_as_a_loop_and_always_warns() {
        let f = ActivityFacts { wake_at: Some(ahead(240)), ..facts() };
        let s = derived(&f);
        assert_eq!(s.activity, Activity::Loop { wakes_in: Duration::from_secs(240) });
        assert!(s.warn, "a session that will restart itself is always worth seeing");
    }

    #[test]
    fn a_wakeup_whose_moment_has_passed_is_not_a_loop() {
        // Either the session is already working again, or the loop ended
        // without saying so. Either way there is nothing to count down to.
        let f = ActivityFacts { wake_at: Some(ago(1)), ..facts() };
        assert!(matches!(derived(&f).activity, Activity::Idle { .. }));
    }

    #[test]
    fn work_happening_now_outranks_a_wakeup_armed_earlier() {
        // A wakeup armed earlier in a turn that is still running has already
        // been superseded; showing a countdown would be describing the past.
        let f = ActivityFacts {
            status: Some(Status::Busy),
            wake_at: Some(ahead(240)),
            shells: vec!["pnpm test".into()],
            ..facts()
        };
        assert_eq!(derived(&f).activity, Activity::Busy);
    }

    #[test]
    fn a_shell_outliving_its_turn_reads_as_a_background_job() {
        let f = ActivityFacts { shells: vec!["pnpm test".into()], ..facts() };
        let s = derived(&f);
        assert_eq!(s.activity, Activity::BgJob);
        assert!(s.warn);
    }

    #[test]
    fn a_loop_outranks_a_background_shell() {
        // Both are unattended, but only the loop starts the model again.
        let f = ActivityFacts {
            shells: vec!["pnpm test".into()],
            wake_at: Some(ahead(30)),
            ..facts()
        };
        assert!(matches!(derived(&f).activity, Activity::Loop { .. }));
    }

    #[test]
    fn a_quiet_session_is_idle_and_only_warns_once_forgotten() {
        assert_eq!(derived(&facts()).activity, Activity::Idle { since: Some(Duration::from_secs(60)) });
        assert!(!derived(&facts()).warn, "waiting for you is the normal case");
        let old =
            ActivityFacts { status_since: Some(ago(FORGOTTEN.as_secs() + 1)), ..facts() };
        assert!(derived(&old).warn, "days of silence is a session you have forgotten");
    }

    #[test]
    fn a_session_with_no_readable_state_is_unknown_not_idle() {
        // An older Claude Code writes no session file. Claiming it is idle
        // would be inventing a fact; claiming it warns would cry wolf.
        let f = ActivityFacts::default();
        assert_eq!(derived(&f).activity, Activity::Unknown);
        assert!(!derived(&f).warn);
    }

    #[test]
    fn subagents_are_counted_not_copied() {
        let f = ActivityFacts {
            status: Some(Status::Busy),
            subagents: vec![
                Subagent { kind: "Explore".into(), model: None, task: "look".into() },
                Subagent { kind: "general-purpose".into(), model: None, task: "build".into() },
            ],
            ..facts()
        };
        assert_eq!(derived(&f).subagents, 2);
    }

    const RECORD: &str = r#"{"pid":9087,"sessionId":"d6b2a4ae","cwd":"/Users/me/p/whirr",
        "status":"busy","statusUpdatedAt":1788253028183,"version":"2.1.252"}"#;

    #[test]
    fn a_session_record_yields_the_pid_transcript_and_status() {
        let r = parse_session_record(RECORD, Path::new("/root")).expect("valid record");
        assert_eq!(r.pid, 9087);
        assert_eq!(r.session_id, "d6b2a4ae");
        assert_eq!(r.cwd, PathBuf::from("/Users/me/p/whirr"));
        assert_eq!(r.status, Some(Status::Busy));
        assert_eq!(r.status_updated_at, Some(UNIX_EPOCH + Duration::from_millis(1788253028183)));
        assert_eq!(r.root, PathBuf::from("/root"));
        assert_eq!(r.version.as_deref(), Some("2.1.252"));
    }

    #[test]
    fn a_record_without_a_status_still_parses() {
        // The status field arrived in a later Claude Code than the pid and
        // cwd did. Dropping the row would lose a session that is running.
        let r = parse_session_record(r#"{"pid":1,"sessionId":"s","cwd":"/p"}"#, Path::new("/r"));
        let r = r.expect("a record with no status is still a session");
        assert_eq!(r.status, None);
        assert_eq!(r.status_updated_at, None);
    }

    #[test]
    fn junk_is_not_a_record() {
        assert!(parse_session_record("", Path::new("/r")).is_none());
        assert!(parse_session_record("{}", Path::new("/r")).is_none());
        assert!(parse_session_record(r#"{"pid":1}"#, Path::new("/r")).is_none());
    }

    #[test]
    fn a_project_directory_flattens_every_separator() {
        assert_eq!(
            project_dir_name(Path::new("/Users/me/Projects/TRAU/TRAU AI")),
            "-Users-me-Projects-TRAU-TRAU-AI"
        );
        assert_eq!(
            project_dir_name(Path::new("/Users/me/my_app.v2")),
            "-Users-me-my-app-v2"
        );
    }

    #[test]
    fn timestamps_convert_to_epoch_seconds() {
        assert_eq!(epoch_secs("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(epoch_secs("2026-09-01T08:57:30.679Z"), Some(1_788_253_050));
        // A March boundary in a leap year, where the civil-calendar shift
        // this uses is easiest to get wrong.
        assert_eq!(epoch_secs("2000-03-01T00:00:00.000Z"), Some(951_868_800));
    }

    #[test]
    fn a_timestamp_that_is_not_the_shape_we_write_is_rejected() {
        assert_eq!(epoch_secs(""), None);
        assert_eq!(epoch_secs("yesterday"), None);
        assert_eq!(epoch_secs("2026-13-01T00:00:00Z"), None, "month 13");
        assert_eq!(epoch_secs("2026-09-00T00:00:00Z"), None, "day 0");
    }

    /// An assistant turn arming a wakeup, `secs` after the epoch.
    fn wake_line(iso: &str, input: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{iso}","message":{{"content":[
               {{"type":"text","text":"ok"}},
               {{"type":"tool_use","name":"ScheduleWakeup","input":{input}}}]}}}}"#
        )
        .replace('\n', "")
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    const T: &str = "2026-09-01T08:57:30.000Z";
    const T_SECS: u64 = 1_788_253_050;

    #[test]
    fn an_armed_wakeup_yields_the_moment_it_fires() {
        let tail = wake_line(T, r#"{"delaySeconds":1200,"reason":"waiting"}"#);
        assert_eq!(armed_wake_at(&tail, at(T_SECS)), Some(at(T_SECS + 1200)));
    }

    #[test]
    fn a_stopped_loop_is_not_armed() {
        // `{"stop": true}` is how a loop ends, and it is logged exactly like
        // the calls that arm one.
        let tail = wake_line(T, r#"{"stop":true}"#);
        assert_eq!(armed_wake_at(&tail, at(T_SECS)), None);
    }

    #[test]
    fn the_last_wakeup_wins() {
        let tail = format!(
            "{}\n{}\n",
            wake_line("2026-09-01T08:00:00.000Z", r#"{"delaySeconds":60}"#),
            wake_line(T, r#"{"stop":true}"#)
        );
        assert_eq!(armed_wake_at(&tail, at(T_SECS)), None, "a later stop ends the loop");
    }

    #[test]
    fn a_transcript_that_kept_growing_has_a_spent_wakeup_not_a_pending_one() {
        // The loop already fired, or was interrupted and never rearmed. A
        // session waiting on a wakeup writes nothing while it waits, so
        // growth well past the call means the record is history.
        let tail = wake_line(T, r#"{"delaySeconds":1200}"#);
        assert_eq!(armed_wake_at(&tail, at(T_SECS + 3600)), None);
        assert!(
            armed_wake_at(&tail, at(T_SECS + 30)).is_some(),
            "the tool result written just after the call must not count as growth"
        );
    }

    /// The record Claude Code writes when a `/schedule` or a cron `/loop`
    /// wakes a session.
    fn fire_line(iso: &str) -> String {
        format!(
            r#"{{"type":"system","subtype":"scheduled_task_fire","timestamp":"{iso}",
               "content":"Running scheduled task (Sep 1 2:47pm)"}}"#
        )
        .replace('\n', "")
    }

    /// The assistant turn that asks for a cron, and the user turn carrying the
    /// result. Only the result knows the new id.
    fn cron_create(call_id: &str, cron_id: &str, schedule: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{T}","message":{{"content":[
               {{"type":"tool_use","id":"{call_id}","name":"CronCreate",
                 "input":{{"cron":"*/5 * * * *","prompt":"check it","recurring":true}}}}]}}}}
               |{{"type":"user","toolUseResult":{{"id":"{cron_id}",
                 "humanSchedule":"{schedule}","recurring":true,"durable":false}},
                 "message":{{"content":[{{"type":"tool_result","tool_use_id":"{call_id}"}}]}}}}"#
        )
        .replace('\n', "")
        .replace("               ", "")
        .replace('|', "\n")
    }

    /// A deletion, call only. Its result says nothing but the id.
    fn cron_delete(cron_id: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{T}","message":{{"content":[
               {{"type":"tool_use","id":"toolu_del","name":"CronDelete",
                 "input":{{"id":"{cron_id}"}}}}]}}}}"#
        )
        .replace('\n', "")
        .replace("               ", "")
    }

    #[test]
    fn a_cron_is_found_with_the_schedule_in_claude_codes_own_words() {
        let u = scan_unattended(&cron_create("toolu_a", "97709169", "Every 5 minutes"));
        assert_eq!(u.created, vec![("97709169".into(), "Every 5 minutes".into())]);
        assert!(u.deleted.is_empty());
    }

    #[test]
    fn a_deletion_is_found_from_the_call_alone() {
        // This is the bug that shipped: deletions were paired with their
        // result, and a CronDelete result does not contain the word "Cron"
        // anywhere, so it never survived the pre-filter and every deleted cron
        // stayed on the card. The call names the id; no result is needed.
        let u = scan_unattended(&cron_delete("97709169"));
        assert_eq!(u.deleted, vec!["97709169".to_string()]);
    }

    #[test]
    fn a_cron_made_then_unmade_then_made_again_leaves_one() {
        // Exactly the shape found on a real machine.
        let text = format!(
            "{}\n{}\n{}",
            cron_create("toolu_a", "97709169", "Every 5 minutes"),
            cron_delete("97709169"),
            cron_create("toolu_b", "190ef941", "Every 5 minutes"),
        );
        let u = scan_unattended(&text);
        let mut live: Vec<String> = u.created.iter().map(|(id, _)| id.clone()).collect();
        live.retain(|id| !u.deleted.contains(id));
        assert_eq!(live, vec!["190ef941".to_string()]);
    }

    #[test]
    fn talking_about_crons_does_not_create_one() {
        let quoting = format!(
            r#"{{"type":"user","message":{{"content":"run CronDelete and CronCreate, humanSchedule"}}}}
               {{"type":"assistant","timestamp":"{T}","message":{{"content":[
                 {{"type":"text","text":"CronDelete takes an id"}}]}}}}"#
        );
        let u = scan_unattended(&quoting);
        assert_eq!(u, Unattended::default());
    }

    #[test]
    fn a_scheduled_fire_is_found_and_the_latest_one_wins() {
        let tail = format!("{}\n{}\n", fire_line("2026-09-01T08:00:00.000Z"), fire_line(T));
        assert_eq!(scan_unattended(&tail).last_fire, Some(at(T_SECS)));
    }

    #[test]
    fn a_transcript_talking_about_scheduled_tasks_has_not_run_one() {
        // This is not hypothetical. A grep for the name across real
        // transcripts "found" four fires in a session that had run none: the
        // matches were its own tool output discussing the record. The parser
        // has to read the record, not the line it sits in.
        let quoting = [
            r#"{"type":"user","timestamp":"2026-09-01T08:57:30.000Z","message":{"content":"grep scheduled_task_fire"}}"#.to_string(),
            r#"{"type":"assistant","timestamp":"2026-09-01T08:57:30.000Z","message":{"content":[{"type":"text","text":"subtype scheduled_task_fire"}]}}"#.to_string(),
            fire_line(T).replace("\"system\"", "\"assistant\""),
            "not json at all, but it mentions scheduled_task_fire".to_string(),
        ];
        for line in quoting {
            assert_eq!(scan_unattended(&line).last_fire, None, "read as a fire: {line}");
        }
    }

    #[test]
    fn a_session_that_takes_scheduled_work_does_not_read_as_idle() {
        // The whole point: a session wired to a cron spends tokens with nobody
        // watching, and "idle" is the one answer that helps least.
        let f = ActivityFacts { last_scheduled_fire: Some(ago(3600)), ..facts() };
        let st = derived(&f);
        assert_eq!(st.activity, Activity::Scheduled { last_fire: Some(Duration::from_secs(3600)) });
        assert!(st.warn, "it will wake without you");
    }

    #[test]
    fn work_happening_now_outranks_a_schedule() {
        // Busy, a loop and a live shell are all happening; a schedule is a
        // thing that will happen. The nearer fact wins the column.
        for nearer in [
            ActivityFacts { status: Some(Status::Busy), ..facts() },
            ActivityFacts { wake_at: Some(ahead(60)), ..facts() },
            ActivityFacts { shells: vec!["pnpm test".into()], ..facts() },
        ] {
            let f = ActivityFacts { last_scheduled_fire: Some(ago(3600)), ..nearer };
            assert!(
                !matches!(derived(&f).activity, Activity::Scheduled { .. }),
                "{:?} should outrank a schedule",
                derived(&f).activity
            );
        }
    }

    #[test]
    fn the_name_alone_does_not_arm_a_loop() {
        // The tool's name appears in listings, system reminders and anything
        // quoting it. Only an assistant's own tool_use arms anything.
        let mentions = [
            r#"{"type":"user","timestamp":"2026-09-01T08:57:30.000Z","message":{"content":"use ScheduleWakeup"}}"#.to_string(),
            r#"{"type":"system","subtype":"tools","tools":["ScheduleWakeup"]}"#.to_string(),
            wake_line(T, r#"{"delaySeconds":1200}"#).replace("tool_use", "text"),
        ];
        for line in mentions {
            assert_eq!(armed_wake_at(&line, at(T_SECS)), None, "armed by: {line}");
        }
    }

    #[test]
    fn a_tail_cut_mid_line_still_finds_the_wakeup() {
        // read_tail slices at a byte offset, so the first line is routinely a
        // fragment. It must be skipped, not poison the scan.
        let tail = format!("ent\":\"garbage\"}}\n{}", wake_line(T, r#"{"delaySeconds":600}"#));
        assert_eq!(armed_wake_at(&tail, at(T_SECS)), Some(at(T_SECS + 600)));
    }

    /// The wrapper the Bash tool builds, in both the shapes seen in the wild.
    fn wrapped(cmd: &str, stdin_closed: bool) -> String {
        let redirect = if stdin_closed { " < /dev/null" } else { "" };
        format!(
            "/bin/zsh -c source /Users/me/.claude/shell-snapshots/snapshot-zsh-1.sh \
             2>/dev/null || true && setopt NO_EXTENDED_GLOB 2>/dev/null || true \
             && eval '{cmd}'{redirect} && pwd -P >| /tmp/claude-bc23-cwd"
        )
    }

    #[test]
    fn the_wrapper_comes_off_whether_or_not_stdin_was_closed() {
        // The redirect is only there for a command that does not read stdin,
        // so anchoring on it fell back to the raw argv for every heredoc.
        for closed in [true, false] {
            assert_eq!(shell_command(&wrapped("CI=true pnpm test", closed)), "CI=true pnpm test");
        }
    }

    #[test]
    fn a_multi_line_command_becomes_one_line() {
        // A heredoc arrives as one argv element with newlines in it, and the
        // dialog draws one line per row.
        let cmd = shell_command(&wrapped("cat <<EOF > f\nhello\nEOF", false));
        assert_eq!(cmd, "cat <<EOF > f hello EOF");
    }

    #[test]
    fn a_command_containing_the_anchor_is_not_cut_short() {
        // The suffix is appended last, so the match is made from the right.
        let cmd = shell_command(&wrapped("echo hi && pwd -P >| /tmp/x", false));
        assert_eq!(cmd, "echo hi && pwd -P >| /tmp/x");
    }

    #[test]
    fn an_argv_that_is_not_a_wrapper_is_handed_back_whole() {
        assert_eq!(shell_command("/bin/zsh -i"), "/bin/zsh -i");
        assert_eq!(shell_command(""), "");
    }

    #[test]
    fn shells_are_counted_and_mcp_servers_are_not() {
        // Every child of a session that is not a shell is an MCP or language
        // server: node, uv, python. Only the shells are Bash-tool jobs.
        let parents: HashMap<i32, i32> = [(20, 9), (21, 9), (22, 9), (30, 7)].into();
        let exec = |pid: i32| {
            Some(PathBuf::from(match pid {
                20 => "/bin/zsh",
                21 => "/opt/homebrew/bin/node",
                22 => "/opt/homebrew/bin/uv",
                _ => "/bin/bash",
            }))
        };
        assert_eq!(shell_children(9, &parents, exec), vec![20]);
        assert_eq!(shell_children(7, &parents, exec), vec![30], "another session's shell");
        assert!(shell_children(999, &parents, exec).is_empty(), "a pid with no children");
    }

    #[test]
    fn an_unreadable_child_is_not_counted_as_a_shell() {
        let parents: HashMap<i32, i32> = [(20, 9)].into();
        assert!(shell_children(9, &parents, |_| None).is_empty());
    }
}
