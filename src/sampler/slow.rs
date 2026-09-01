use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime};

use super::ports::PortRow;
use super::{claude_state, ports, sessions};
use super::{PortInfo, SlowSnap, Snapshot};

const TICK: Duration = Duration::from_secs(10);

/// Walk every pid, keep the Claude processes, and read what the session card
/// needs. `exec_path` is one cheap syscall per pid — deliberately not `args`,
/// whose argv buffer would make a full-system walk expensive.
/// Ask the hosting terminal what it calls each session, and attach that.
///
/// One `ps` and one host query per tick, and only when there are sessions to
/// label. Hosts that cannot answer cheaply return nothing and the rows keep
/// the project name — reading titles out of AppleScript would mean *waiting*
/// on it, which can block for minutes.
fn attach_titles(sessions: &mut [sessions::ClaudeSession], parents: &HashMap<i32, i32>) {
    if sessions.is_empty() {
        return;
    }
    // Detected per session, not once for the whole list. Sessions can live in
    // different terminals at the same time, and asking only the first one's
    // host meant a single session in a terminal that cannot answer stripped
    // the titles off every other row.
    //
    // Cached by bundle so the host is still only *queried* once per distinct
    // terminal, however many sessions it holds.
    let mut by_host: BTreeMap<std::path::PathBuf, Vec<crate::host::Surface>> = BTreeMap::new();
    for s in sessions.iter_mut() {
        let Some(tty) = s.tty.clone() else { continue };
        let Some(host) = crate::host::detect_with(s.pid, parents) else { continue };
        let surfaces = by_host
            .entry(host.bundle.clone())
            .or_insert_with(|| crate::host::surfaces(&host));
        match surfaces.iter().find(|x| x.tty == tty) {
            Some(m) => {
                s.jumpable = true;
                if !m.title.is_empty() {
                    s.title = Some(m.title.clone());
                }
            }
            // A scriptable terminal cannot be asked cheaply whether it holds
            // this tty, so it is taken at its word; anything else offers no
            // way to select a tab and must not advertise one.
            None => {
                s.jumpable = matches!(host.kind, crate::host::HostKind::AppleScript { .. })
            }
        }
    }
}

// Reading Claude Code's state off disk. The decisions live next door in
// `claude_state`, which stays pure the way `sessions` does: everything here
// returns facts, and nothing here decides anything.

/// Every config root under `home`: `~/.claude` and any sibling a second
/// account uses. A root is a directory holding a `projects` directory —
/// `~/.claude.json` is a file and several `~/.claude*.bak` files exist, and
/// neither is a tree to read sessions out of.
fn config_roots(home: &Path) -> Vec<PathBuf> {
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
/// Where a session's transcript lives, and the directory beside it that holds
/// its subagents.
///
/// The encoded name is tried first because it is pure arithmetic on the cwd.
/// The fallback exists because that encoding has changed before and a session
/// whose directory does not match must not silently lose its state — one
/// `read_dir` of a directory with a handful of entries is cheap enough to pay
/// only when the cheap answer misses.
fn transcript_of(rec: &claude_state::SessionRecord) -> Option<PathBuf> {
    let projects = rec.root.join("projects");
    let file = format!("{}.jsonl", rec.session_id);
    let guess = projects.join(claude_state::project_dir_name(&rec.cwd)).join(&file);
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
/// The subagents a session is running right now, newest first.
///
/// `dir` is the `subagents` directory beside the transcript. A missing
/// directory means the session has never spawned one, which is none of them,
/// not an error.
///
/// Only the transcripts still being written to are read: a subagent writes
/// continuously while it runs and never again once it finishes, so mtime
/// separates the live ones from a session's whole history of them without
/// opening a single file. The small `.meta.json` beside each live one is then
/// worth reading; the transcript itself never is.
fn hot_subagents(dir: &Path, now: SystemTime) -> Vec<claude_state::Subagent> {
    let mut live: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| {
            let at = e.metadata().and_then(|m| m.modified()).ok()?;
            (now.duration_since(at).ok()? <= claude_state::SUBAGENT_HOT).then(|| (at, e.path()))
        })
        .collect();
    live.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    live
        .iter()
        .map(|(_, p)| {
            // A subagent whose meta cannot be read is still a subagent that is
            // running. Dropping it would quietly shrink the count on the card,
            // which is the one number the card promises.
            std::fs::read_to_string(p.with_extension("meta.json"))
                .ok()
                .and_then(|t| claude_state::parse_subagent(&t))
                .unwrap_or_default()
        })
        .collect()
}
/// The last `bytes` of a file, as text, for a tail scan that must not read a
/// multi-megabyte transcript from the front.
fn read_tail(path: &Path, bytes: u64) -> Option<String> {
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

/// How much of a transcript's tail to read looking for an armed wakeup.
///
/// A session waiting on one is quiet, so the call that armed it is within a
/// few kilobytes of the end — everything written after it is the tool result
/// and a couple of bookkeeping records. Reading the whole file would mean
/// scanning tens of megabytes of history every tick for a record that is
/// either at the end or absent.
const TAIL: u64 = 64 * 1024;

/// What the tail of a transcript says about work the session will do on its
/// own: an armed wakeup, and the last scheduled task to have woken it.
///
/// One read serving both. They are found by different parsers but live in the
/// same few kilobytes, and reading the file twice to ask two questions of it
/// would be the expensive half of this tick done twice.
fn unattended(transcript: &Path) -> (Option<SystemTime>, Option<SystemTime>) {
    let Some(tail) = read_tail(transcript, TAIL) else { return (None, None) };
    let grew_until = std::fs::metadata(transcript).and_then(|m| m.modified()).ok();
    let wake = grew_until.and_then(|t| claude_state::armed_wake_at(&tail, t));
    (wake, claude_state::last_scheduled_fire(&tail))
}

/// Attach what each session is doing: busy, looping, running a background
/// shell, or waiting for you.
///
/// A session whose state cannot be read keeps `Unknown` and stays on the card.
/// Losing a row because its config root moved would be a worse failure than
/// showing it without a state — the row is still a session you can jump to.
fn attach_state(
    sessions: &mut [sessions::ClaudeSession],
    parents: &HashMap<i32, i32>,
    scheduled: &mut HashMap<String, SystemTime>,
) {
    if sessions.is_empty() {
        return;
    }
    let roots = std::env::var_os("HOME")
        .map(|h| config_roots(Path::new(&h)))
        .unwrap_or_default();
    // Every root's session files, read once per tick: a few hundred bytes
    // each, one per session that is running.
    let records: HashMap<i32, claude_state::SessionRecord> = roots
        .iter()
        .flat_map(|root| {
            std::fs::read_dir(root.join("sessions"))
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .filter_map(move |e| {
                    let text = std::fs::read_to_string(e.path()).ok()?;
                    claude_state::parse_session_record(&text, root)
                })
        })
        .map(|r| (r.pid, r))
        .collect();
    let now = SystemTime::now();
    for s in sessions.iter_mut() {
        // One argv read per shell, and only for sessions that have one: the
        // call copies a 256 KB argument area and has no business anywhere
        // near a full-system walk.
        s.facts.shells = claude_state::shell_children(s.pid, parents, crate::mac::proc::exec_path)
            .into_iter()
            .filter_map(crate::mac::proc::args)
            .map(|argv| claude_state::shell_command(&argv))
            .collect();
        let Some(rec) = records.get(&s.pid) else { continue };
        s.facts.status = rec.status;
        s.facts.status_since = rec.status_updated_at;
        if let Some(transcript) = transcript_of(rec) {
            // `<session id>.jsonl` and `<session id>/subagents` are siblings,
            // so dropping the extension gets from one to the other.
            let dir = transcript.with_extension("").join("subagents");
            s.facts.subagents = hot_subagents(&dir, now);
            // The heartbeat of a working session. One stat, and the only
            // thing that separates a long turn from a hung one.
            s.facts.last_write =
                std::fs::metadata(&transcript).and_then(|m| m.modified()).ok();
            // Only a quiet session can be waiting on a wakeup, and this is
            // the one read in the tick that touches a large file — so the
            // session actively writing to its transcript is exactly the one
            // that never pays for it.
            if rec.status != Some(claude_state::Status::Busy) {
                let (wake, fired) = unattended(&transcript);
                s.facts.wake_at = wake;
                // Remembered rather than re-read each tick. The fire record is
                // written at the *start* of a turn, so a long one pushes it out
                // of the tail and the session would go back to reading "idle",
                // which is the one thing a session that wakes on a schedule is
                // not.
                if let Some(t) = fired {
                    scheduled.insert(rec.session_id.clone(), t);
                }
            }
            s.facts.last_scheduled_fire = scheduled.get(&rec.session_id).copied();
        }
        s.record = Some(rec.clone());
    }
}

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

/// Is `cwd` inside a git repository — its own root, or any directory above it?
///
/// The old test was a single `cwd/.git` probe, which answered a much narrower
/// question: "is this the repo *root*". A dev server started from a package
/// inside a monorepo failed it and dropped to the Others card.
///
/// `stop_at` bounds the walk and is never itself examined. It is the user's
/// home directory in practice, because a `~/.git` from dotfiles-in-git would
/// otherwise make every process on the machine look like a dev server.
///
/// Costs one `stat` per level rather than one per pid, still only on the 10s
/// tick, and path depth bounds the loop.
fn in_git_repo(cwd: &Path, stop_at: Option<&Path>) -> bool {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if stop_at == Some(d) {
            return false;
        }
        // A worktree or submodule has `.git` as a file, not a directory —
        // `exists` accepts both, and both mean "a checkout lives here".
        if d.join(".git").exists() {
            return true;
        }
        dir = d.parent();
    }
    false
}

/// One port scan. `None` means the scan failed; see `usable_stdout`.
fn scan_ports() -> Option<Vec<PortRow>> {
    let out = Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"])
        .output()
        .ok()?;
    let stdout = usable_stdout(&out)?;
    let ports = parse_lsof(&String::from_utf8_lossy(stdout));
    // Read once per scan, not once per pid. Absent `HOME` just means an
    // unbounded walk to `/`, which is the old behaviour plus the walk-up.
    let home = std::env::var_os("HOME").map(PathBuf::from);
    Some(ports::build_rows(&ports, |pid| {
        let cwd = crate::mac::proc::cwd(pid);
        // A handful of stats per unique pid per 10s tick; build_rows already
        // guarantees this closure runs once per pid.
        let is_git = cwd.as_deref().is_some_and(|c| in_git_repo(c, home.as_deref()));
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
    // Sessions seen taking a scheduled task, by session id. See `attach_state`
    // for why this is remembered rather than read fresh every tick.
    let mut scheduled: HashMap<String, SystemTime> = HashMap::new();
    loop {
        // Ports and sessions are independent sources: sessions come from a
        // full pid walk that never consults lsof. Scanning them separately is
        // what keeps an lsof failure — or a machine with no listening sockets
        // at all — from blanking the sessions card.
        let mut sessions = scan_sessions();
        // One `ps` shared by both passes: the host lookup walks parents up to
        // the terminal, the state lookup walks them down to the shells.
        let parents = crate::host::parent_map();
        attach_titles(&mut sessions, &parents);
        attach_state(&mut sessions, &parents, &mut scheduled);
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
    use super::{in_git_repo, parse_lsof, usable_stdout};
    use std::os::unix::process::ExitStatusExt;
    use std::path::{Path, PathBuf};
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

    // `in_git_repo` is tested against this checkout rather than a fixture
    // tree: it exists to answer a question about the real filesystem, and a
    // temp-dir mock would only prove the mock was built right. whirr's own
    // repo root is `CARGO_MANIFEST_DIR`, which every one of these leans on.
    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn a_repo_root_is_in_a_repo() {
        assert!(in_git_repo(&repo_root(), None));
    }

    #[test]
    fn a_subdirectory_of_a_repo_is_in_that_repo() {
        // The monorepo case, and the whole point of the change: `.git` lives
        // at the root only, so `cd apps/web && npm run dev` used to read as
        // "not a project".
        assert!(in_git_repo(&repo_root().join("src/ui"), None));
    }

    #[test]
    fn a_path_outside_any_repo_is_not_in_one() {
        assert!(!in_git_repo(Path::new("/"), None));
    }

    #[test]
    fn the_walk_stops_below_the_boundary() {
        // The dotfiles guard. Plenty of people keep `~` under git; if the walk
        // were allowed to reach it, every process whose cwd is anywhere under
        // home would classify as a dev server and the localhost card would
        // fill with junk.
        let root = repo_root();
        assert!(
            !in_git_repo(&root.join("src/ui"), Some(&root)),
            "a repo at the boundary must not count — that is the ~/.git case"
        );
    }

    #[test]
    fn the_boundary_does_not_hide_a_nearer_repo() {
        // Stopping at home must not stop *early*: a real project below the
        // boundary still has to be found.
        assert!(in_git_repo(&repo_root().join("src/ui"), Some(Path::new("/"))));
    }
}


#[cfg(test)]
mod live {
    /// What this machine's own sessions report, which no fixture can stand in
    /// for: every state here comes from files and processes that only exist at
    /// runtime. It also parses every subagent record the machine has ever
    /// written, which is the only way to know the fixtures match what Claude
    /// Code actually produces rather than what it produced once.
    ///
    /// `cargo test --lib live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn show_this_machines_sessions() {
        let mut sessions = super::scan_sessions();
        let parents = crate::host::parent_map();
        super::attach_state(&mut sessions, &parents, &mut Default::default());
        let now = std::time::SystemTime::now();
        for s in &sessions {
            let st = super::claude_state::state(&s.facts, now);
            println!(
                "{:>6} {:<28} {:<9} {:?} subagents={} shells={:?} warn={}",
                s.pid,
                s.project,
                s.tty.as_deref().unwrap_or("-"),
                st.activity,
                st.subagents,
                s.facts.shells,
                st.warn
            );
        }
        assert!(!sessions.is_empty(), "whirr's own session should be running this");
        // And every real subagent record this machine has ever written, to
        // prove the fixture matches what Claude Code actually produces.
        let home = std::env::var("HOME").expect("HOME");
        let mut seen = 0;
        for root in super::config_roots(std::path::Path::new(&home)) {
            for meta in glob_meta(&root.join("projects")) {
                let text = std::fs::read_to_string(&meta).expect("readable");
                let parsed = super::claude_state::parse_subagent(&text);
                assert!(parsed.is_some(), "unparsed subagent meta: {}", meta.display());
                seen += 1;
                if seen <= 3 {
                    println!("subagent {:?}", parsed.unwrap());
                }
            }
        }
        println!("parsed {seen} real subagent records");
        // And every transcript on this machine, to check the fire parser
        // against the real records and, more usefully, the real near-misses:
        // a transcript quotes the record's name constantly, so the count of
        // mentions is nothing like the count of fires.
        let (mut fired, mut files, mut mentioned) = (0, 0, 0);
        for root in super::config_roots(std::path::Path::new(&home)) {
            for project in std::fs::read_dir(root.join("projects")).into_iter().flatten().flatten() {
                for f in std::fs::read_dir(project.path()).into_iter().flatten().flatten() {
                    let path = f.path();
                    if path.extension().is_none_or(|e| e != "jsonl") {
                        continue;
                    }
                    files += 1;
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    let hits = text.matches("scheduled_task_fire").count();
                    mentioned += hits;
                    if super::claude_state::last_scheduled_fire(&text).is_some() {
                        fired += 1;
                    }
                }
            }
        }
        println!(
            "{fired} of {files} transcripts ran a scheduled task; \
             {mentioned} lines mention one"
        );
    }

    /// Every `*.meta.json` under a projects tree.
    fn glob_meta(projects: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for project in std::fs::read_dir(projects).into_iter().flatten().flatten() {
            for session in std::fs::read_dir(project.path()).into_iter().flatten().flatten() {
                let dir = session.path().join("subagents");
                for f in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                    let p = f.path();
                    if p.to_string_lossy().ends_with(".meta.json") {
                        out.push(p);
                    }
                }
            }
        }
        out
    }
}
