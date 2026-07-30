//! Purpose-grouping for listening ports: which are the user's dev servers,
//! which are Claude Code sessions, and which are background noise.
//!
//! Pure by design — the filesystem test for `.git` and the argv/cwd syscalls
//! happen in `slow.rs` and arrive here as `ProcFacts`, so the heuristic is
//! table-testable. See
//! `docs/superpowers/specs/2026-07-30-whirr-grouped-ports-design.md`.

use std::collections::HashMap;
use std::path::PathBuf;

use super::PortInfo;

/// Claude Code installs versioned binaries under this path fragment, so the
/// executable name itself is a version number ("2.1.220") and useless as a
/// label. If Claude ever moves, sessions fall back to `Localhost` — they run in
/// git repos — which `claude_path_shape_is_pinned` is there to make visible.
const CLAUDE_PATH: &str = "/claude/versions/";

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
    if facts.exec_path.as_deref().is_some_and(|p| p.contains(CLAUDE_PATH)) {
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
    rows.sort_by_key(|r| (r.group.rank(), r.ports[0]));
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

    // `PortInfo` still carries `project` at this point; Task 3 removes both the
    // field and this line. Do not remove the field early — `slow.rs` still
    // populates it until then and the crate would not build.
    fn pi(port: u16, process: &str, pid: i32) -> PortInfo {
        PortInfo { port, process: process.to_string(), pid, project: None }
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
    fn claude_path_shape_is_pinned() {
        // If Claude Code changes where versioned binaries live, this test fails
        // loudly instead of sessions silently reclassifying as Localhost.
        assert_eq!(CLAUDE_PATH, "/claude/versions/");
        let real = "/Users/me/.local/share/claude/versions/2.1.220";
        assert!(real.contains(CLAUDE_PATH), "observed layout must still match");
    }
}
