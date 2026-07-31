//! Claude Code sessions, sourced from processes rather than listening sockets.
//!
//! A session holds a TCP socket only sometimes — measured on one machine, 8
//! sessions were running and only 4 were listening — so a port-sourced list is
//! incomplete by construction. Enumerating processes is what makes it whole.
//!
//! Pure by design: the syscalls live in `slow.rs` and arrive here as
//! `SessionFacts`. See
//! `docs/superpowers/specs/2026-07-30-whirr-three-cards-design.md`.

use std::path::PathBuf;

/// What `slow.rs` reads per candidate pid.
pub struct SessionFacts {
    pub pid: i32,
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
    /// Controlling terminal, e.g. `ttys021`. This is what distinguishes two
    /// sessions in the same project: it says which pane to go to.
    pub tty: Option<String>,
}

/// Keep the Claude processes and turn them into rows, ordered by project, then
/// tty, then pid. Sorting by pid last keeps the card stable between ticks when
/// two sessions are otherwise identical.
pub fn build_sessions(facts: &[SessionFacts]) -> Vec<ClaudeSession> {
    let mut out: Vec<ClaudeSession> = facts
        .iter()
        .filter(|f| {
            f.exec_path
                .as_deref()
                .and_then(|p| p.to_str())
                .is_some_and(super::ports::is_claude)
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
            exec_path: Some(PathBuf::from(exec)),
            cwd: cwd.map(PathBuf::from),
            tty: tty.map(String::from),
        }
    }

    const CLAUDE: &str = "/Users/me/.local/bin/claude";

    #[test]
    fn non_claude_processes_are_excluded() {
        let facts = [
            f(1, "/opt/homebrew/bin/node", Some("/Users/me/p/app"), Some("ttys001")),
            f(2, CLAUDE, Some("/Users/me/p/app"), Some("ttys002")),
        ];
        let s = build_sessions(&facts);
        assert_eq!(s.len(), 1, "only the claude process is a session");
        assert_eq!(s[0].pid, 2);
    }

    #[test]
    fn sessions_without_a_listening_port_are_still_included() {
        // The whole point of sourcing from processes: no port information is
        // consulted at all, so a session that holds no socket still appears.
        let s = build_sessions(&[f(9, CLAUDE, Some("/Users/me/p/whirr"), Some("ttys009"))]);
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
        let s = build_sessions(&facts);
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
    fn a_session_with_no_readable_cwd_is_still_listed() {
        let s = build_sessions(&[f(4, CLAUDE, None, Some("ttys004"))]);
        assert_eq!(s.len(), 1, "an unreadable cwd must not drop the session");
        assert!(!s[0].project.is_empty(), "must still have some label");
    }

    #[test]
    fn ordering_is_deterministic_when_project_and_tty_match() {
        // Two sessions indistinguishable by project and tty must still come back
        // in a stable order, so the card does not reshuffle between ticks.
        let facts = [
            f(20, CLAUDE, Some("/Users/me/p/x"), None),
            f(10, CLAUDE, Some("/Users/me/p/x"), None),
        ];
        for _ in 0..5 {
            let pids: Vec<i32> = build_sessions(&facts).iter().map(|s| s.pid).collect();
            assert_eq!(pids, vec![10, 20], "must fall back to pid order");
        }
    }
}
