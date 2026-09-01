//! Claude Code sessions, sourced from processes rather than listening sockets.
//!
//! A session holds a TCP socket only sometimes — measured on one machine, 8
//! sessions were running and only 4 were listening — so a port-sourced list is
//! incomplete by construction. Enumerating processes is what makes it whole.
//!
//! Pure by design: the syscalls live in `slow.rs` and arrive here as
//! `SessionFacts`. See
//! `docs/superpowers/specs/2026-07-30-whirr-three-cards-design.md`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::claude_state::{ActivityFacts, SessionRecord};

/// What `slow.rs` reads per candidate pid.
#[derive(Clone)]
pub struct SessionFacts {
    pub pid: i32,
    /// Whether this process is Claude Code. Decided by the scanner, not here:
    /// a session whose binary the updater has deleted has no path left to
    /// judge, and the answer then comes from its name instead.
    pub is_claude: bool,
    pub exec_path: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub tty: Option<String>,
}

/// One running Claude Code session.
#[derive(Clone, Debug)]
pub struct ClaudeSession {
    pub pid: i32,
    /// Project directory basename — the thing the user recognises.
    pub project: String,
    /// What the hosting terminal calls this session, when it can say. cmux
    /// titles a workspace with the task the session is working on, which is
    /// worth far more than a project directory or a tty.
    pub title: Option<String>,
    /// Whether the host can actually put this session's tab in front.
    ///
    /// False when no surface matches, which does happen — cmux does not
    /// report one for every tty. Merely activating the application does not
    /// count: whirr is often running inside that same app, so it looks
    /// identical to a key that does nothing.
    pub jumpable: bool,
    /// Controlling terminal, e.g. `ttys021`. This is what distinguishes two
    /// sessions in the same project: it says which pane to go to.
    pub tty: Option<String>,
    /// What `slow.rs` observed about what this session is doing. Raw
    /// observations, not conclusions: `claude_state::state` turns them into
    /// an activity against the `now` of whichever frame asks.
    pub facts: ActivityFacts,
    /// Claude Code's own record of the session, when there is one to read.
    ///
    /// Carried whole rather than copied field by field: everything the
    /// details dialog wants — the full path, the account, the build, when it
    /// started — is already here, and a parallel struct holding a subset
    /// would only be a second thing to keep in step.
    pub record: Option<SessionRecord>,
}

/// Does `pid` have another Claude Code process above it?
///
/// The chain runs through whatever is in between — the daemon's child is a pty
/// host, whose child is a background session — so this walks parents rather
/// than checking one. `seen` bounds it: a parent map read from a live system
/// can contain a cycle if a pid was reused between rows.
fn descends_from_claude(pid: i32, parents: &HashMap<i32, i32>, claude: &HashSet<i32>) -> bool {
    // Seeded with the pid itself: a cycle that comes back round would
    // otherwise find the process as its own ancestor and drop it.
    let mut seen = HashSet::from([pid]);
    let mut cur = pid;
    while let Some(&parent) = parents.get(&cur) {
        if parent <= 1 || !seen.insert(parent) {
            return false;
        }
        if claude.contains(&parent) {
            return true;
        }
        cur = parent;
    }
    false
}

/// Keep the Claude processes and turn them into rows, ordered by project, then
/// tty, then pid. Sorting by pid last keeps the card stable between ticks when
/// two sessions are otherwise identical.
pub fn build_sessions(facts: &[SessionFacts], parents: &HashMap<i32, i32>) -> Vec<ClaudeSession> {
    let claude: HashSet<i32> = facts.iter().filter(|f| f.is_claude).map(|f| f.pid).collect();
    let mut out: Vec<ClaudeSession> = facts
        .iter()
        .filter(|f| {
            f.is_claude
                // A session is the one you started. Claude Code runs plenty of
                // other processes from the same binary and under it: a
                // transient daemon, a `--bg-pty-host` out of the app bundle,
                // and the background sessions those spawn. They were listed as
                // sessions of their own, named after whatever directory they
                // began in, answering no key and coming and going on their own.
                //
                // Descent is what separates them, not a terminal: the spawned
                // ones are handed pseudo-terminals by the pty host and even
                // get their own session files, so both of those tests let them
                // through. Nothing you started has another Claude Code above
                // it.
                && !descends_from_claude(f.pid, parents, &claude)
                // And nothing you started is without a terminal. This catches
                // a helper whose parent has died and left it reparented, where
                // there is no longer a chain to walk.
                && f.tty.is_some()
        })
        .map(|f| ClaudeSession {
            pid: f.pid,
            project: f
                .cwd
                .as_deref()
                .and_then(|c| c.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                // An unreadable cwd must not hide a session; the pid is at
                // least something the user can act on.
                .unwrap_or_else(|| format!("pid {}", f.pid)),
            tty: f.tty.clone(),
            // Both filled in by `slow.rs` once the host has been asked.
            title: None,
            jumpable: false,
            facts: ActivityFacts::default(),
            record: None,
        })
        .collect();
    out.sort_by(|a, b| {
        a.project
            .cmp(&b.project)
            .then_with(|| a.tty.cmp(&b.tty))
            .then_with(|| a.pid.cmp(&b.pid))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn f(pid: i32, exec: &str, cwd: Option<&str>, tty: Option<&str>) -> SessionFacts {
        SessionFacts {
            pid,
            is_claude: super::super::ports::is_claude(exec),
            exec_path: Some(PathBuf::from(exec)),
            cwd: cwd.map(PathBuf::from),
            tty: tty.map(String::from),
        }
    }

    const CLAUDE: &str = "/Users/me/.local/bin/claude";

    /// Sessions with nothing above them, which is the ordinary case.
    fn build_sessions_no_parents(facts: &[SessionFacts]) -> Vec<ClaudeSession> {
        build_sessions(facts, &HashMap::new())
    }

    #[test]
    fn non_claude_processes_are_excluded() {
        let facts = [
            f(1, "/opt/homebrew/bin/node", Some("/Users/me/p/app"), Some("ttys001")),
            f(2, CLAUDE, Some("/Users/me/p/app"), Some("ttys002")),
        ];
        let s = build_sessions(&facts, &HashMap::new());
        assert_eq!(s.len(), 1, "only the claude process is a session");
        assert_eq!(s[0].pid, 2);
    }

    #[test]
    fn sessions_without_a_listening_port_are_still_included() {
        // The whole point of sourcing from processes: no port information is
        // consulted at all, so a session that holds no socket still appears.
        let s = build_sessions_no_parents(&[f(9, CLAUDE, Some("/Users/me/p/whirr"), Some("ttys009"))]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].project, "whirr");
    }

    #[test]
    fn two_sessions_in_one_project_are_adjacent_and_ordered_by_tty() {
        let facts = [
            f(2, CLAUDE, Some("/Users/me/p/axterio"), Some("ttys021")),
            f(1, CLAUDE, Some("/Users/me/p/axterio"), Some("ttys020")),
            f(3, CLAUDE, Some("/Users/me/p/beta"), Some("ttys003")),
        ];
        let s = build_sessions(&facts, &HashMap::new());
        let got: Vec<(&str, Option<&str>)> =
            s.iter().map(|x| (x.project.as_str(), x.tty.as_deref())).collect();
        assert_eq!(
            got,
            vec![
                ("axterio", Some("ttys020")),
                ("axterio", Some("ttys021")),
                ("beta", Some("ttys003")),
            ],
            "same project must be adjacent, ordered by tty"
        );
    }

    #[test]
    fn a_claude_spawned_by_another_claude_is_not_a_session() {
        // The chain seen on a real machine: a session starts a transient
        // daemon, the daemon starts a pty host, and the pty host starts
        // background sessions. Those are handed pseudo-terminals and write
        // their own session files, so neither a tty nor a session file tells
        // them apart. Nothing you started has another Claude Code above it.
        let facts = [
            f(37464, CLAUDE, Some("/Users/me/p/app"), Some("ttys010")),
            f(24684, CLAUDE, Some("/Users/me"), None),
            f(24725, "/Users/me/.local/share/claude/ClaudeCode.app/Contents/MacOS/claude", Some("/Users/me/p/app"), None),
            f(25049, CLAUDE, Some("/Users/me/p/app"), Some("ttys033")),
            f(3012, "/bin/zsh", Some("/Users/me"), Some("ttys010")),
        ];
        let parents: HashMap<i32, i32> =
            [(37464, 3012), (24684, 37464), (24725, 24684), (25049, 24725), (3012, 3010)].into();
        let s = build_sessions(&facts, &parents);
        let pids: Vec<i32> = s.iter().map(|x| x.pid).collect();
        assert_eq!(pids, vec![37464], "only the one you started");
    }

    #[test]
    fn a_cycle_in_the_parent_map_does_not_hang_the_scan() {
        // `ps` is read row by row on a live system, so a reused pid can leave
        // a chain that points back at itself.
        let facts = [f(10, CLAUDE, Some("/Users/me/p/app"), Some("ttys001"))];
        let parents: HashMap<i32, i32> = [(10, 20), (20, 30), (30, 10)].into();
        assert_eq!(build_sessions(&facts, &parents).len(), 1);
    }

    #[test]
    fn a_claude_process_with_no_terminal_is_a_helper_not_a_session() {
        // `claude daemon run` and the `--bg-pty-host` out of the app bundle
        // are the same binary and were being listed as sessions, named after
        // whatever directory they started in.
        let helpers = [
            f(30, CLAUDE, Some("/Users/me"), None),
            f(31, "/Users/me/.local/share/claude/ClaudeCode.app/Contents/MacOS/claude", Some("/Users/me/p/app"), None),
        ];
        assert!(build_sessions_no_parents(&helpers).is_empty(), "no terminal, no session");
        // And the real one beside them still comes through.
        let mixed = [helpers[0].clone(), f(32, CLAUDE, Some("/Users/me/p/app"), Some("ttys010"))];
        let s = build_sessions_no_parents(&mixed);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].pid, 32);
    }

    #[test]
    fn a_session_with_no_readable_cwd_is_still_listed() {
        let s = build_sessions_no_parents(&[f(4, CLAUDE, None, Some("ttys004"))]);
        assert_eq!(s.len(), 1, "an unreadable cwd must not drop the session");
        assert!(!s[0].project.is_empty(), "must still have some label");
    }

    #[test]
    fn ordering_is_deterministic_when_project_and_tty_match() {
        // Two sessions indistinguishable by project and tty must still come back
        // in a stable order, so the card does not reshuffle between ticks.
        let facts = [
            f(20, CLAUDE, Some("/Users/me/p/x"), Some("ttys001")),
            f(10, CLAUDE, Some("/Users/me/p/x"), Some("ttys001")),
        ];
        for _ in 0..5 {
            let pids: Vec<i32> = build_sessions(&facts, &HashMap::new()).iter().map(|s| s.pid).collect();
            assert_eq!(pids, vec![10, 20], "must fall back to pid order");
        }
    }
}
