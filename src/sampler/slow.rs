use std::collections::BTreeMap;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::Duration;

use super::ports::PortRow;
use super::{ports, sessions};
use super::{PortInfo, SlowSnap, Snapshot};

const TICK: Duration = Duration::from_secs(10);

/// Walk every pid, keep the Claude processes, and read what the session card
/// needs. `exec_path` is one cheap syscall per pid — deliberately not `args`,
/// whose argv buffer would make a full-system walk expensive.
fn scan_sessions() -> Vec<sessions::ClaudeSession> {
    let facts: Vec<sessions::SessionFacts> = crate::mac::proc::list_all_pids()
        .into_iter()
        .filter_map(|pid| {
            let exec_path = crate::mac::proc::exec_path(pid)?;
            if !exec_path.to_str().is_some_and(ports::is_claude) {
                return None;
            }
            // Only matched pids pay for the extra two calls.
            let info = crate::mac::proc::bsd_info(pid);
            Some(sessions::SessionFacts {
                pid,
                exec_path: Some(exec_path),
                cwd: crate::mac::proc::cwd(pid),
                tty: info.and_then(|i| i.tty),
            })
        })
        .collect();
    sessions::build_sessions(&facts)
}

pub fn parse_lsof(output: &str) -> Vec<PortInfo> {
    let mut by_port: BTreeMap<u16, PortInfo> = BTreeMap::new();
    let (mut pid, mut cmd) = (0i32, String::new());
    for line in output.lines() {
        match line.split_at_checked(1) {
            Some(("p", rest)) => pid = rest.parse().unwrap_or(0),
            Some(("c", rest)) => cmd = rest.to_string(),
            Some(("n", rest)) => {
                if let Some(port) = rest.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
                    if pid != 0 && !cmd.is_empty() {
                        by_port.entry(port).or_insert_with(|| PortInfo {
                            port,
                            process: cmd.clone(),
                            pid,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    by_port.into_values().collect()
}

pub fn run(tx: Sender<Snapshot>) {
    let mut last_good: Vec<PortRow> = Vec::new();
    loop {
        let output = Command::new("lsof")
            .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"])
            .output();
        let snap = match output {
            Ok(out) if out.status.success() || !out.stdout.is_empty() => {
                let ports = parse_lsof(&String::from_utf8_lossy(&out.stdout));
                let rows = ports::build_rows(&ports, |pid| {
                    let cwd = crate::mac::proc::cwd(pid);
                    // One stat per unique pid per 10s tick; build_rows already
                    // guarantees this closure runs once per pid.
                    let is_git = cwd.as_deref().is_some_and(|c| c.join(".git").exists());
                    ports::ProcFacts {
                        exec_path: crate::mac::proc::args(pid).map(|a| a.exec_path),
                        cwd,
                        is_git,
                    }
                });
                last_good = rows;
                SlowSnap { rows: last_good.clone(), sessions: scan_sessions(), stale: false }
            }
            // lsof exits with status 1 and empty stdout when nothing matches
            // the filter (e.g. no listening TCP sockets right now) — that is
            // a genuinely empty result, not a failure, so it must not fall
            // into the stale arm below (which would otherwise report "stale"
            // forever on a box with zero listeners). Only treat it as an
            // error if stderr has something to say.
            Ok(out)
                if out.status.code() == Some(1)
                    && out.stdout.is_empty()
                    && out.stderr.is_empty() =>
            {
                last_good = Vec::new();
                SlowSnap { rows: Vec::new(), sessions: Vec::new(), stale: false }
            }
            _ => SlowSnap { rows: last_good.clone(), sessions: Vec::new(), stale: true },
        };
        if tx.send(Snapshot::Slow(snap)).is_err() {
            return;
        }
        std::thread::sleep(TICK);
    }
}

// These tests cover `parse_lsof` only. `run()`'s exit-status/staleness
// handling (see the match above — status 1 + empty stdout/stderr is a valid
// empty result, not staleness) isn't unit-tested here because it shells out
// to the real `lsof` binary; it's exercised manually / via the app.
#[cfg(test)]
mod tests {
    use super::parse_lsof;

    const FIXTURE: &str = "\
p512
cpostgres
f7
n127.0.0.1:5432
f8
n[::1]:5432
p9001
cnode
f22
n*:3000
p9002
cControl Center
f10
n*:7000
";

    #[test]
    fn parses_and_dedups_ports() {
        let ports = parse_lsof(FIXTURE);
        let view: Vec<(u16, &str, i32)> =
            ports.iter().map(|p| (p.port, p.process.as_str(), p.pid)).collect();
        assert_eq!(
            view,
            vec![(3000, "node", 9001), (5432, "postgres", 512), (7000, "Control Center", 9002)]
        );
    }

    #[test]
    fn ignores_garbage() {
        assert!(parse_lsof("").is_empty());
        assert!(parse_lsof("nonsense\nlines\n").is_empty());
    }
}

