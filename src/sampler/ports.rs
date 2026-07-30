//! Purpose-grouping for listening ports: which are the user's dev servers,
//! which are Claude Code sessions, and which are background noise.
//!
//! Pure by design — the filesystem test for `.git` and the argv/cwd syscalls
//! happen in `slow.rs` and arrive here as `ProcFacts`, so the heuristic is
//! table-testable. See
//! `docs/superpowers/specs/2026-07-30-whirr-grouped-ports-design.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::PortInfo;

/// Claude Code installs versioned binaries under this path fragment, so the
/// executable name itself is a version number ("2.1.220") and useless as a
/// label — that's why detection keys on `exec_path` instead. `exec_path` is
/// the launcher path the user actually invoked (from `KERN_PROCARGS2`), not
/// the resolved binary, so on current installs it is a stable `~/.local/bin/claude`
/// rather than a versioned path. Accept both shapes: the current launcher
/// (file name exactly `claude`) and the older versioned layout.
pub(crate) fn is_claude(exec_path: &str) -> bool {
    Path::new(exec_path).file_name().is_some_and(|n| n == "claude")
        || exec_path.contains("/claude/versions/")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PortGroup {
    Localhost,
    Claude,
    Other,
}

impl PortGroup {
    /// Display order: most actionable first.
    fn rank(self) -> u8 {
        match self {
            PortGroup::Localhost => 0,
            PortGroup::Claude => 1,
            PortGroup::Other => 2,
        }
    }
}

/// One process and every port it listens on.
#[derive(Clone, Debug)]
pub struct PortRow {
    pub group: PortGroup,
    /// Project basename for `Localhost`/`Claude`; process name for `Other`.
    pub label: String,
    pub pid: i32,
    /// Ascending.
    pub ports: Vec<u16>,
}

/// What `slow.rs` reads per pid. `is_git` is supplied rather than derived so
/// this module stays free of filesystem access.
pub struct ProcFacts {
    pub exec_path: Option<String>,
    pub cwd: Option<PathBuf>,
    pub is_git: bool,
}

/// Claude wins over Localhost: sessions also run inside git repositories.
pub fn classify(facts: &ProcFacts) -> PortGroup {
    if facts.exec_path.as_deref().is_some_and(is_claude) {
        return PortGroup::Claude;
    }
    // `is_git` is meaningless without a cwd to have tested.
    if facts.cwd.is_some() && facts.is_git {
        return PortGroup::Localhost;
    }
    PortGroup::Other
}

/// Collapse `ports` to one row per pid, classify each, and order by group then
/// lowest port. `lookup` is called once per unique pid.
pub fn build_rows(
    ports: &[PortInfo],
    mut lookup: impl FnMut(i32) -> ProcFacts,
) -> Vec<PortRow> {
    let mut by_pid: HashMap<i32, PortRow> = HashMap::new();
    for p in ports {
        match by_pid.get_mut(&p.pid) {
            Some(row) => row.ports.push(p.port),
            None => {
                let facts = lookup(p.pid);
                let group = classify(&facts);
                let project = facts
                    .cwd
                    .as_deref()
                    .and_then(|c| c.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                // Others have no meaningful project, so they keep the name lsof
                // reported; the first two groups are identified by project.
                let label = match group {
                    PortGroup::Other => p.process.clone(),
                    _ => project.unwrap_or_else(|| p.process.clone()),
                };
                by_pid.insert(p.pid, PortRow { group, label, pid: p.pid, ports: vec![p.port] });
            }
        }
    }
    let mut rows: Vec<PortRow> = by_pid.into_values().collect();
    for row in &mut rows {
        row.ports.sort_unstable();
    }
    rows.sort_by_key(|r| (r.group.rank(), r.ports[0], r.pid));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn facts(exec: Option<&str>, cwd: Option<&str>, is_git: bool) -> ProcFacts {
        ProcFacts {
            exec_path: exec.map(String::from),
            cwd: cwd.map(PathBuf::from),
            is_git,
        }
    }

    #[test]
    fn claude_is_detected_by_exec_path() {
        let f = facts(
            Some("/Users/me/.local/share/claude/versions/2.1.220"),
            Some("/Users/me/Documents/Projects/axterio"),
            true,
        );
        // A Claude session also lives in a git repo — claude must win.
        assert_eq!(classify(&f), PortGroup::Claude);
    }

    #[test]
    fn a_git_cwd_is_localhost() {
        let f = facts(Some("/usr/local/bin/node"), Some("/Users/me/Projects/app"), true);
        assert_eq!(classify(&f), PortGroup::Localhost);
    }

    #[test]
    fn a_non_git_cwd_is_other() {
        let f = facts(Some("/usr/libexec/rapportd"), Some("/"), false);
        assert_eq!(classify(&f), PortGroup::Other);
    }

    #[test]
    fn unreadable_cwd_is_other() {
        assert_eq!(classify(&facts(None, None, false)), PortGroup::Other);
        // is_git must not be trusted when there is no cwd to have tested.
        assert_eq!(classify(&facts(None, None, true)), PortGroup::Other);
    }

    fn pi(port: u16, process: &str, pid: i32) -> PortInfo {
        PortInfo { port, process: process.to_string(), pid }
    }

    #[test]
    fn ports_of_one_process_collapse_into_a_single_row() {
        let ports = [pi(6006, "node", 50), pi(4206, "node", 50), pi(63643, "node", 50)];
        let rows = build_rows(&ports, |_| {
            facts(Some("/bin/node"), Some("/Users/me/Projects/glassbook-frontend"), true)
        });
        assert_eq!(rows.len(), 1, "one process must yield one row");
        assert_eq!(rows[0].ports, vec![4206, 6006, 63643], "ports ascending");
        assert_eq!(rows[0].label, "glassbook-frontend", "label is the project, not the process");
        assert_eq!(rows[0].pid, 50);
    }

    #[test]
    fn rows_are_ordered_localhost_then_claude_then_other() {
        let ports = [
            pi(5000, "ControlCenter", 1),
            pi(65067, "2.1.220", 2),
            pi(3000, "next-server", 3),
        ];
        let rows = build_rows(&ports, |pid| match pid {
            1 => facts(Some("/System/ControlCenter"), Some("/"), false),
            2 => facts(Some("/x/claude/versions/2.1.220"), Some("/Users/me/p/axterio"), true),
            _ => facts(Some("/bin/next-server"), Some("/Users/me/p/axterio"), true),
        });
        let groups: Vec<PortGroup> = rows.iter().map(|r| r.group).collect();
        assert_eq!(
            groups,
            vec![PortGroup::Localhost, PortGroup::Claude, PortGroup::Other]
        );
        // Same project in two groups is expected and must not be merged.
        assert_eq!(rows[0].label, "axterio");
        assert_eq!(rows[1].label, "axterio");
    }

    #[test]
    fn other_rows_are_labelled_with_the_process_name() {
        let rows = build_rows(&[pi(5000, "ControlCenter", 1)], |_| facts(None, None, false));
        assert_eq!(rows[0].label, "ControlCenter", "no project, so use the process name");
    }

    #[test]
    fn rows_within_a_group_are_ordered_by_lowest_port() {
        let ports = [pi(9000, "a", 1), pi(3000, "b", 2), pi(5000, "c", 3)];
        let rows = build_rows(&ports, |_| facts(Some("/bin/x"), Some("/Users/me/p/proj"), true));
        let firsts: Vec<u16> = rows.iter().map(|r| r.ports[0]).collect();
        assert_eq!(firsts, vec![3000, 5000, 9000]);
    }

    #[test]
    fn lookup_is_called_once_per_pid() {
        let mut calls = 0;
        let ports = [pi(1, "x", 7), pi(2, "x", 7), pi(3, "x", 7)];
        let _ = build_rows(&ports, |_| {
            calls += 1;
            facts(Some("/bin/x"), Some("/Users/me/p/proj"), true)
        });
        assert_eq!(calls, 1, "one syscall batch per pid, not per port");
    }

    #[test]
    fn claude_path_shapes_are_pinned_to_real_observed_paths() {
        // These are real paths, not hand-written assumptions: the first is the
        // current launcher install that was silently falling through to
        // Localhost before this fix; the second is the older versioned layout.
        assert!(is_claude("/Users/j.salmik/.local/bin/claude"));
        assert!(is_claude("/Users/me/.local/share/claude/versions/2.1.187"));

        // Negative cases proving the rule is not over-broad: a file name that
        // merely contains "claude" as a substring must not match.
        assert!(!is_claude("/usr/local/bin/claude-helper"));
        assert!(!is_claude("/opt/notclaude"));
    }

    #[test]
    fn a_claude_session_in_a_git_repo_classifies_as_claude_not_localhost() {
        // Reproduces the live-machine bug: exec_path is the invoked launcher
        // path (~/.local/bin/claude), and the session's cwd is a git repo, so
        // the old `/claude/versions/` substring rule fell through to
        // Localhost and the Claude group rendered empty.
        let f = facts(
            Some("/Users/j.salmik/.local/bin/claude"),
            Some("/Users/j.salmik/Documents/Projects/axterio"),
            true,
        );
        assert_eq!(classify(&f), PortGroup::Claude);
    }

    #[test]
    fn deterministic_ordering_with_tied_sort_keys() {
        // Two pids in the same group with the same lowest port create a tie
        // in the (group, lowest_port) sort key. This tie cannot arise through
        // parse_lsof (which deduplicates ports via BTreeMap), but this module
        // should not silently depend on that invariant. The test documents that
        // build_rows independently enforces determinism via pid as a tertiary key.
        let ports = [
            pi(3000, "app1", 100),
            pi(3000, "app2", 200),
        ];

        // Run build_rows several times to check for non-determinism.
        let mut results = Vec::new();
        for _ in 0..5 {
            let rows = build_rows(&ports, |_| {
                facts(Some("/bin/node"), Some("/Users/me/p/proj"), true)
            });
            let pids: Vec<i32> = rows.iter().map(|r| r.pid).collect();
            results.push(pids);
        }

        // All runs must produce identical order.
        for result in &results[1..] {
            assert_eq!(result, &results[0], "ordering must be deterministic across runs");
        }

        // The order must be sorted by pid (100 before 200).
        assert_eq!(results[0], vec![100, 200], "rows must be ordered by pid when sort keys tie");
    }
}
