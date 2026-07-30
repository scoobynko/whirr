use std::collections::{BTreeMap, HashMap};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::mac::proc::ProcArgs;
use super::{PortInfo, SlowSnap, Snapshot};

const TICK: Duration = Duration::from_secs(10);

/// Max rendered length of a smart label.
const LABEL_MAX: usize = 28;

/// Path components that carry no identity when walking up from a
/// version-named executable (`…/claude/versions/2.1.187` → "claude").
const GENERIC_DIRS: [&str; 7] =
    ["versions", "bin", "sbin", "libexec", "contents", "macos", "current"];

/// Interpreters whose own name says little — the argv usually says more.
const INTERPRETERS: [&str; 11] = [
    "node", "python", "python3", "bun", "deno", "ruby", "java", "perl", "sh", "bash", "zsh",
];

/// `2.1.187`, `v18.2.0`, … — optional `v` + at least two dot-separated
/// numeric parts.
fn version_like(s: &str) -> bool {
    let s = s.strip_prefix('v').unwrap_or(s);
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A short "what is this really?" label for a listening process: real app
/// name (resolved past version-named binaries) plus a hint from its
/// arguments. Falls back to the raw lsof name when args are unreadable
/// (other users' processes / system daemons).
pub fn smart_label(raw: &str, args: Option<&ProcArgs>) -> String {
    let Some(pa) = args else {
        return raw.to_string();
    };
    let exec_base = basename(&pa.exec_path);
    let argv0_base = pa.argv.first().map(|a| basename(a)).unwrap_or_default();

    let name = if version_like(exec_base) {
        // Walk ancestors past generic/version components to something real.
        pa.exec_path
            .split('/')
            .rev()
            .skip(1)
            .find(|c| {
                c.len() > 1
                    && !version_like(c)
                    && !GENERIC_DIRS.contains(&c.to_lowercase().as_str())
            })
            .unwrap_or(raw)
    } else if INTERPRETERS.contains(&exec_base)
        && !argv0_base.is_empty()
        && argv0_base != exec_base
    {
        // e.g. npm running under the node binary.
        argv0_base
    } else {
        exec_base
    };

    let hint = pa.argv.get(1).map(|first| {
        if first.starts_with('-') {
            // Flag: keep its name, drop any =value.
            first.split('=').next().unwrap_or(first).to_string()
        } else {
            // Script / subcommand: basename, plus one more short word.
            let mut h = basename(first).to_string();
            if let Some(second) = pa.argv.get(2) {
                if !second.starts_with('-') && h.len() + second.len() < 18 {
                    h.push(' ');
                    h.push_str(second);
                }
            }
            h
        }
    });

    let label = match hint {
        Some(h) if !h.is_empty() => format!("{name} {h}"),
        _ => name.to_string(),
    };
    label.chars().take(LABEL_MAX).collect()
}

/// Rewrite each port's process name with its smart label, calling `lookup`
/// once per unique pid (a process often listens on several ports).
pub fn apply_smart_labels(
    ports: &mut [PortInfo],
    mut lookup: impl FnMut(i32) -> Option<ProcArgs>,
) {
    let mut cache: HashMap<i32, Option<ProcArgs>> = HashMap::new();
    for p in ports.iter_mut() {
        let pa = cache.entry(p.pid).or_insert_with(|| lookup(p.pid));
        p.process = smart_label(&p.process, pa.as_ref());
    }
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
                enrich_projects(&mut ports, |pid| {
                    crate::mac::proc::cwd(pid)
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                });
                apply_smart_labels(&mut ports, crate::mac::proc::args);
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
    use super::{apply_smart_labels, enrich_projects, parse_lsof, smart_label};
    use crate::mac::proc::ProcArgs;

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

    fn pa(exec: &str, argv: &[&str]) -> ProcArgs {
        ProcArgs {
            exec_path: exec.to_string(),
            argv: argv.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn smart_label_resolves_version_named_binary() {
        let a = pa(
            "/Users/x/.local/share/claude/versions/2.1.187",
            &["/Users/x/.local/share/claude/versions/2.1.187", "--bg-spare", "/tmp/x.sock"],
        );
        assert_eq!(smart_label("2.1.187", Some(&a)), "claude --bg-spare");
    }

    #[test]
    fn smart_label_shows_interpreter_script() {
        let a = pa("/usr/local/bin/node", &["node", "server.js"]);
        assert_eq!(smart_label("node", Some(&a)), "node server.js");
    }

    #[test]
    fn smart_label_uses_argv0_for_interpreter_wrappers() {
        let a = pa("/usr/local/bin/node", &["npm", "run", "dev"]);
        assert_eq!(smart_label("node", Some(&a)), "npm run dev");
    }

    #[test]
    fn smart_label_strips_flag_values() {
        let a = pa("/opt/thing/bin/thing", &["thing", "--port=8080"]);
        assert_eq!(smart_label("thing", Some(&a)), "thing --port");
    }

    #[test]
    fn smart_label_falls_back_to_raw() {
        assert_eq!(smart_label("rapportd", None), "rapportd");
        // Version-named exec with no meaningful ancestor falls back to raw.
        let a = pa("/2.0.1", &["/2.0.1"]);
        assert_eq!(smart_label("mystery", Some(&a)), "mystery");
    }

    #[test]
    fn smart_label_truncates() {
        let a = pa(
            "/usr/bin/supercalifragilistic-server-daemon",
            &["supercalifragilistic-server-daemon", "--enable-extremely-verbose-logging"],
        );
        assert!(smart_label("x", Some(&a)).chars().count() <= 28);
    }

    #[test]
    fn apply_smart_labels_caches_per_pid() {
        let mut ports = parse_lsof(FIXTURE);
        let mut calls = 0;
        apply_smart_labels(&mut ports, |pid| {
            calls += 1;
            (pid == 9001).then(|| pa("/apps/webserver/versions/1.2.3", &["1.2.3", "--serve"]))
        });
        assert_eq!(calls, 3); // one lookup per unique pid
        assert_eq!(ports.iter().find(|p| p.port == 3000).unwrap().process, "webserver --serve");
        assert_eq!(ports.iter().find(|p| p.port == 5432).unwrap().process, "postgres");
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

