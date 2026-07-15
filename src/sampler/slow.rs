use std::collections::{BTreeMap, HashMap};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::Duration;

use super::{PortInfo, SlowSnap, Snapshot};

const TICK: Duration = Duration::from_secs(10);

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
                            project: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    by_port.into_values().collect()
}

/// Fill `project` via `lookup`, calling it once per unique pid (a process
/// often listens on several ports).
pub fn enrich_projects(
    ports: &mut [PortInfo],
    mut lookup: impl FnMut(i32) -> Option<String>,
) {
    let mut cache: HashMap<i32, Option<String>> = HashMap::new();
    for p in ports.iter_mut() {
        p.project = cache.entry(p.pid).or_insert_with(|| lookup(p.pid)).clone();
    }
}

pub fn run(tx: Sender<Snapshot>) {
    let mut last_good: Vec<PortInfo> = Vec::new();
    loop {
        let output = Command::new("lsof")
            .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"])
            .output();
        let snap = match output {
            Ok(out) if out.status.success() || !out.stdout.is_empty() => {
                let mut ports = parse_lsof(&String::from_utf8_lossy(&out.stdout));
                enrich_projects(&mut ports, crate::mac::proc::cwd_basename);
                last_good = ports;
                SlowSnap { ports: last_good.clone(), stale: false }
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
                SlowSnap { ports: Vec::new(), stale: false }
            }
            _ => SlowSnap { ports: last_good.clone(), stale: true },
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
    use super::{parse_lsof, enrich_projects};

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
        let view: Vec<(u16, &str, i32, Option<&str>)> = ports
            .iter()
            .map(|p| (p.port, p.process.as_str(), p.pid, p.project.as_deref()))
            .collect();
        assert_eq!(
            view,
            vec![
                (3000, "node", 9001, None),
                (5432, "postgres", 512, None),
                (7000, "Control Center", 9002, None)
            ]
        );
    }

    #[test]
    fn ignores_garbage() {
        assert!(parse_lsof("").is_empty());
        assert!(parse_lsof("nonsense\nlines\n").is_empty());
    }

    #[test]
    fn enrich_fills_project_per_pid_once() {
        let mut ports = parse_lsof(FIXTURE);
        let mut calls = 0;
        enrich_projects(&mut ports, |pid| {
            calls += 1;
            (pid == 9001).then(|| "my-app".to_string())
        });
        assert_eq!(calls, 3); // one lookup per unique pid
        assert_eq!(
            ports.iter().find(|p| p.port == 3000).unwrap().project.as_deref(),
            Some("my-app")
        );
        assert_eq!(ports.iter().find(|p| p.port == 5432).unwrap().project, None);
    }
}
