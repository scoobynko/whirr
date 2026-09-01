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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A busy flag older than this is not a session working hard, it is a session
/// that died mid-turn. Claude Code rewrites `statusUpdatedAt` on every
/// transition, and no single turn stays busy this long.
const STUCK: Duration = Duration::from_secs(30 * 60);

/// Idle this long is a session you have forgotten about. It costs nothing in
/// itself, but it is holding its MCP servers and language servers open.
const FORGOTTEN: Duration = Duration::from_secs(24 * 60 * 60);

/// A subagent transcript touched this recently is one still being written to.
/// Subagents write continuously while they run and never again once they
/// finish, so file mtime answers "is one out right now" without a read.
const SUBAGENT_HOT: Duration = Duration::from_secs(60);

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
    /// Quiet, but a shell it started is still running.
    BgJob,
    /// Waiting for a human. `since` is how long, when that is knowable.
    Idle { since: Option<Duration> },
    /// No session file and nothing else to go on — an older Claude Code, or a
    /// config root whirr cannot read.
    Unknown,
}

/// Everything `state` needs, gathered by the caller so the decision itself
/// stays pure and testable.
#[derive(Clone, Debug, Default)]
pub struct ActivityFacts {
    pub status: Option<Status>,
    /// Age of that status, i.e. how long the session has been in it.
    pub status_age: Option<Duration>,
    /// Live shell children of the session process.
    pub shells: usize,
    /// Subagent transcripts being written to right now.
    pub subagents: usize,
    /// Time until an armed wakeup fires, when one is armed.
    pub wakes_in: Option<Duration>,
}

/// A session's activity and whether it is worth pulling the eye to.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionState {
    pub activity: Activity,
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
pub fn state(f: &ActivityFacts) -> SessionState {
    let activity = if f.status == Some(Status::Busy) {
        Activity::Busy
    } else if let Some(w) = f.wakes_in {
        Activity::Loop { wakes_in: w }
    } else if f.shells > 0 {
        Activity::BgJob
    } else if f.status == Some(Status::Idle) {
        Activity::Idle { since: f.status_age }
    } else {
        // Nothing said it is idle; it may simply be unreadable.
        Activity::Unknown
    };
    let warn = match &activity {
        // Not "busy for a while" — busy for longer than any turn lasts, which
        // means the flag outlived the process writing it.
        Activity::Busy => f.status_age.is_some_and(|d| d >= STUCK),
        Activity::Loop { .. } => true,
        Activity::BgJob => true,
        Activity::Idle { since } => since.is_some_and(|d| d >= FORGOTTEN),
        Activity::Unknown => false,
    };
    SessionState { activity, subagents: f.subagents, warn }
}

/// One running session as `sessions/<pid>.json` describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRecord {
    pub pid: i32,
    pub session_id: String,
    pub cwd: PathBuf,
    pub status: Option<Status>,
    pub status_updated_at: Option<SystemTime>,
    /// The config root this record was found under, so the transcript can be
    /// looked up in the same tree.
    pub root: PathBuf,
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
        root: root.to_path_buf(),
    })
}

/// Every config root under `home`: `~/.claude` and any sibling a second
/// account uses. A root is a directory holding a `projects` directory —
/// `~/.claude.json` is a file and several `~/.claude*.bak` files exist, and
/// neither is a tree to read sessions out of.
pub fn config_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(".claude"))
                && p.join("projects").is_dir()
        })
        .collect();
    roots.sort();
    roots
}

/// The directory name Claude Code gives a project: its path with every
/// separator, dot and space flattened to a dash.
pub fn project_dir_name(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' || c == ' ' || c == '_' { '-' } else { c })
        .collect()
}

/// Where a session's transcript lives, and the directory beside it that holds
/// its subagents.
///
/// The encoded name is tried first because it is pure arithmetic on the cwd.
/// The fallback exists because that encoding has changed before and a session
/// whose directory does not match must not silently lose its state — one
/// `read_dir` of a directory with a handful of entries is cheap enough to pay
/// only when the cheap answer misses.
pub fn transcript_of(rec: &SessionRecord) -> Option<PathBuf> {
    let projects = rec.root.join("projects");
    let file = format!("{}.jsonl", rec.session_id);
    let guess = projects.join(project_dir_name(&rec.cwd)).join(&file);
    if guess.is_file() {
        return Some(guess);
    }
    std::fs::read_dir(&projects)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join(&file))
        .find(|p| p.is_file())
}

/// How many of a session's subagents are being written to right now.
///
/// `dir` is the `subagents` directory beside the transcript. A missing
/// directory means the session has never spawned one, which is zero, not an
/// error.
pub fn hot_subagents(dir: &Path, now: SystemTime) -> usize {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter(|e| {
            e.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .is_some_and(|age| age <= SUBAGENT_HOT)
        })
        .count()
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

/// How many of `pid`'s children are shells.
///
/// This is what a background `Bash` call looks like from outside: the tool
/// runs `/bin/zsh -c …` as a child of the session and leaves it running. The
/// session's other children are MCP servers and language servers, which are
/// node, python or uv — never a shell — so the executable name alone
/// separates them, without paying for an argv read per child.
pub fn shell_children(pid: i32, parents: &HashMap<i32, i32>, exec: impl Fn(i32) -> Option<PathBuf>) -> usize {
    parents
        .iter()
        .filter(|(_, &ppid)| ppid == pid)
        .filter(|(&child, _)| exec(child).as_deref().is_some_and(is_shell))
        .count()
}

fn is_shell(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("zsh" | "bash" | "sh" | "dash" | "fish")
    )
}

/// The last `bytes` of a file, as text, for a tail scan that must not read a
/// multi-megabyte transcript from the front.
pub fn read_tail(path: &Path, bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(bytes))).ok()?;
    let mut buf = Vec::with_capacity(bytes.min(len) as usize);
    f.read_to_end(&mut buf).ok()?;
    // The cut lands mid-line and mid-character; both are the caller's
    // problem to skip, and a lossy decode keeps it to a broken first line.
    Some(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> ActivityFacts {
        ActivityFacts { status: Some(Status::Idle), status_age: Some(Duration::from_secs(60)), ..Default::default() }
    }

    #[test]
    fn a_busy_session_reads_as_busy() {
        let f = ActivityFacts { status: Some(Status::Busy), ..facts() };
        assert_eq!(state(&f).activity, Activity::Busy);
        assert!(!state(&f).warn, "working is the normal case, not a warning");
    }

    #[test]
    fn a_busy_flag_older_than_any_turn_is_a_stuck_session() {
        // Claude Code rewrites statusUpdatedAt on every transition, so a busy
        // flag this old belongs to a process that died mid-turn.
        let f = ActivityFacts {
            status: Some(Status::Busy),
            status_age: Some(STUCK + Duration::from_secs(1)),
            ..facts()
        };
        assert_eq!(state(&f).activity, Activity::Busy);
        assert!(state(&f).warn, "a busy flag that outlived its turn must be flagged");
    }

    #[test]
    fn an_armed_wakeup_reads_as_a_loop_and_always_warns() {
        let f = ActivityFacts { wakes_in: Some(Duration::from_secs(240)), ..facts() };
        let s = state(&f);
        assert_eq!(s.activity, Activity::Loop { wakes_in: Duration::from_secs(240) });
        assert!(s.warn, "a session that will restart itself is always worth seeing");
    }

    #[test]
    fn work_happening_now_outranks_a_wakeup_armed_earlier() {
        // A wakeup armed earlier in a turn that is still running has already
        // been superseded; showing a countdown would be describing the past.
        let f = ActivityFacts {
            status: Some(Status::Busy),
            wakes_in: Some(Duration::from_secs(240)),
            shells: 2,
            ..facts()
        };
        assert_eq!(state(&f).activity, Activity::Busy);
    }

    #[test]
    fn a_shell_outliving_its_turn_reads_as_a_background_job() {
        let f = ActivityFacts { shells: 1, ..facts() };
        let s = state(&f);
        assert_eq!(s.activity, Activity::BgJob);
        assert!(s.warn);
    }

    #[test]
    fn a_loop_outranks_a_background_shell() {
        // Both are unattended, but only the loop starts the model again.
        let f = ActivityFacts { shells: 1, wakes_in: Some(Duration::from_secs(30)), ..facts() };
        assert!(matches!(state(&f).activity, Activity::Loop { .. }));
    }

    #[test]
    fn a_quiet_session_is_idle_and_only_warns_once_forgotten() {
        assert_eq!(
            state(&facts()).activity,
            Activity::Idle { since: Some(Duration::from_secs(60)) }
        );
        assert!(!state(&facts()).warn, "waiting for you is the normal case");
        let old = ActivityFacts { status_age: Some(FORGOTTEN + Duration::from_secs(1)), ..facts() };
        assert!(state(&old).warn, "days of silence is a session you have forgotten");
    }

    #[test]
    fn a_session_with_no_readable_state_is_unknown_not_idle() {
        // An older Claude Code writes no session file. Claiming it is idle
        // would be inventing a fact; claiming it warns would cry wolf.
        let f = ActivityFacts { status: None, status_age: None, ..Default::default() };
        assert_eq!(state(&f).activity, Activity::Unknown);
        assert!(!state(&f).warn);
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
        assert_eq!(shell_children(9, &parents, exec), 1);
        assert_eq!(shell_children(7, &parents, exec), 1, "another session's shell");
        assert_eq!(shell_children(999, &parents, exec), 0, "a pid with no children");
    }

    #[test]
    fn an_unreadable_child_is_not_counted_as_a_shell() {
        let parents: HashMap<i32, i32> = [(20, 9)].into();
        assert_eq!(shell_children(9, &parents, |_| None), 0);
    }
}
