use std::collections::BTreeMap;
use std::process::{Command, Output};
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
            Some(sessions::SessionFacts {
                pid,
                exec_path: Some(exec_path),
                cwd: crate::mac::proc::cwd(pid),
                tty: crate::mac::proc::tty(pid).flatten(),
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

/// The listing an `lsof` run produced, or `None` if the run failed and the
/// caller should keep showing its last good rows as stale.
///
/// lsof exits with status 1 and nothing on either stream when nothing matches
/// the filter (e.g. no listening TCP sockets right now). That is a genuinely
/// empty result, not a failure, so it must not be reported as stale — a box
/// with zero listeners would otherwise say "stale" forever.
fn usable_stdout(out: &Output) -> Option<&[u8]> {
    if out.status.success() || !out.stdout.is_empty() {
        Some(&out.stdout)
    } else if out.status.code() == Some(1) && out.stdout.is_empty() && out.stderr.is_empty() {
        Some(&[])
    } else {
        None
    }
}

/// One port scan. `None` means the scan failed; see `usable_stdout`.
fn scan_ports() -> Option<Vec<PortRow>> {
    let out = Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"])
        .output()
        .ok()?;
    let stdout = usable_stdout(&out)?;
    let ports = parse_lsof(&String::from_utf8_lossy(stdout));
    Some(ports::build_rows(&ports, |pid| {
        let cwd = crate::mac::proc::cwd(pid);
        // One stat per unique pid per 10s tick; build_rows already
        // guarantees this closure runs once per pid.
        let is_git = cwd.as_deref().is_some_and(|c| c.join(".git").exists());
        ports::ProcFacts {
            // `exec_path`, not `args`: one cheap proc_pidpath call instead of
            // a kern.argmax-sized buffer per pid. Both yield a path
            // `ports::is_claude` recognises — see its doc comment.
            exec_path: crate::mac::proc::exec_path(pid)
                .map(|p| p.to_string_lossy().into_owned()),
            cwd,
            is_git,
        }
    }))
}

pub fn run(tx: Sender<Snapshot>) {
    let mut last_good: Vec<PortRow> = Vec::new();
    loop {
        // Ports and sessions are independent sources: sessions come from a
        // full pid walk that never consults lsof. Scanning them separately is
        // what keeps an lsof failure — or a machine with no listening sockets
        // at all — from blanking the sessions card.
        let sessions = scan_sessions();
        let (rows, stale) = match scan_ports() {
            Some(rows) => {
                last_good = rows;
                (last_good.clone(), false)
            }
            None => (last_good.clone(), true),
        };
        if tx.send(Snapshot::Slow(SlowSnap { rows, sessions, stale })).is_err() {
            return;
        }
        std::thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_lsof, usable_stdout};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    /// An `lsof` result with the given exit code and streams.
    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            // `from_raw` takes a wait(2) status word: the exit code is the
            // high byte, so `code << 8` is "exited with `code`".
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_successful_run_is_usable() {
        assert_eq!(usable_stdout(&output(0, "p1\n", "")), Some(&b"p1\n"[..]));
    }

    #[test]
    fn no_listening_sockets_is_an_empty_result_not_a_failure() {
        // lsof's way of saying "nothing matched". Reporting this as stale
        // would pin a box with zero listeners at "stale" forever.
        assert_eq!(usable_stdout(&output(1, "", "")), Some(&b""[..]));
    }

    #[test]
    fn a_real_failure_is_not_usable() {
        assert_eq!(usable_stdout(&output(1, "", "lsof: permission denied")), None);
        assert_eq!(usable_stdout(&output(127, "", "")), None);
    }

    #[test]
    fn partial_output_with_a_bad_status_is_still_usable() {
        // lsof commonly warns about unreadable pids and exits non-zero while
        // still listing everything it could see.
        let out = output(1, "p1\ncnode\nn*:3000\n", "lsof: WARNING: can't stat()");
        assert_eq!(usable_stdout(&out), Some(&b"p1\ncnode\nn*:3000\n"[..]));
    }

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

