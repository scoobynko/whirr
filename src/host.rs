//! The terminal application a Claude session is running inside, and what can
//! be asked of it.
//!
//! Two capabilities, both keyed on the same thing: **the tty**. cmux,
//! Terminal.app and iTerm2 all identify their tabs or panes by controlling
//! terminal, and the sessions card already carries one per row.
//!
//! - *Focus* — bring the session's tab to the front (#27).
//! - *Label* — use the host's own name for the session instead of a tty (#28).
//!
//! Hosts that offer neither still get the last rung of the ladder: activating
//! the application. It does not select the tab, but with a single window it is
//! most of the value, and it works for terminals nobody has written an adapter
//! for — including ones that do not exist yet.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A terminal application, and how much can be asked of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostKind {
    /// Has a CLI that reports every surface with its tty, title and pane, and
    /// can focus a pane by reference.
    Cmux { cli: PathBuf },
    /// Scriptable, and exposes `tty` per tab. Focus only: reading a title back
    /// would mean *waiting* on AppleScript, which can block for minutes.
    AppleScript { app: &'static str },
    /// Unknown: the app can still be brought to the front.
    Opaque,
}

/// The application hosting a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    /// Path to the `.app` bundle, used for the activate-only fallback.
    pub bundle: PathBuf,
    pub kind: HostKind,
}

/// One tab/pane as the host describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    pub tty: String,
    /// What the host calls it. Frequently far more useful than a tty — cmux
    /// titles a workspace with the task the session is working on.
    pub title: String,
    /// The handle `focus` needs. Opaque to whirr.
    pub pane: String,
}

/// pid → ppid for every process on the machine.
///
/// Read from `ps` rather than `proc_pidinfo`, which is the whole reason this
/// function exists: a session's chain runs `claude` → `zsh` → `login` → the
/// terminal, and `login` is setuid root. `proc_pidinfo` cannot read a
/// root-owned process's parent without privileges, so the walk stopped one
/// step short of the terminal — every time, on every host. `ps` reads the
/// same links through `sysctl` and is not so restricted.
///
/// One subprocess, measured at 20ms wall and no measurable CPU, and only on
/// the keypress that needs it.
pub fn parent_map() -> HashMap<i32, i32> {
    Command::new("ps")
        .args(["-Ao", "pid=,ppid="])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_parents(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// Two whitespace-separated columns per line; anything else is skipped.
pub fn parse_parents(text: &str) -> HashMap<i32, i32> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let ppid = it.next()?.parse().ok()?;
            Some((pid, ppid))
        })
        .collect()
}

/// Walk up from `pid` until a process is found inside an `.app` bundle.
///
/// A session's chain is typically `claude` → `zsh` → `login` → the terminal.
pub fn detect_with(pid: i32, parents: &HashMap<i32, i32>) -> Option<Host> {
    /// Deep enough for any real chain; a bound rather than trusting that the
    /// parent links never form a cycle.
    const MAX_DEPTH: usize = 32;
    let mut pid = pid;
    for _ in 0..MAX_DEPTH {
        let parent = *parents.get(&pid)?;
        if parent <= 1 || parent == pid {
            return None;
        }
        pid = parent;
        if let Some(exe) = crate::mac::proc::exec_path(pid) {
            if let Some(bundle) = bundle_of(&exe) {
                let kind = classify(&bundle);
                return Some(Host { bundle, kind });
            }
        }
    }
    None
}

/// `detect_with`, reading the process table itself.
pub fn detect(pid: i32) -> Option<Host> {
    detect_with(pid, &parent_map())
}

/// The `.app` bundle an executable lives in, if any.
///
/// `/Applications/cmux.app/Contents/MacOS/cmux` → `/Applications/cmux.app`.
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let mut dir = exe.parent();
    while let Some(d) = dir {
        if d.extension().is_some_and(|e| e == "app") {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// What can be asked of the app at `bundle`.
fn classify(bundle: &Path) -> HostKind {
    let name = bundle.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match name.as_str() {
        "cmux" => {
            // The CLI ships inside the bundle and is not on PATH by default,
            // so it is found relative to the app whirr just identified rather
            // than hoped for in the environment.
            let cli = bundle.join("Contents/Resources/bin/cmux");
            if cli.exists() {
                HostKind::Cmux { cli }
            } else {
                HostKind::Opaque
            }
        }
        "Terminal" => HostKind::AppleScript { app: "Terminal" },
        "iTerm2" | "iTerm" => HostKind::AppleScript { app: "iTerm2" },
        _ => HostKind::Opaque,
    }
}

/// Pull every surface out of `cmux tree --all`.
///
/// The tree is drawn for humans, so this reads it positionally: a `pane` line
/// establishes the pane that the `surface` lines beneath it belong to.
///
/// ```text
/// ├── workspace workspace:3 "Ownit Produkce [FE]"
/// │   ├── pane pane:3
/// │   │   └── surface surface:3 [terminal] "✳ Switch to dev branch" [selected] tty=ttys010
/// ```
pub fn parse_cmux_tree(text: &str) -> Vec<Surface> {
    let mut out = Vec::new();
    let mut pane = String::new();
    for line in text.lines() {
        if let Some(rest) = after(line, "pane pane:") {
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !n.is_empty() {
                pane = format!("pane:{n}");
            }
        }
        let Some(rest) = after(line, "tty=") else { continue };
        let tty: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
        if tty.is_empty() || pane.is_empty() {
            continue;
        }
        // The title is the first quoted run on the line. A title containing a
        // quote would truncate here, which costs a nicer label and nothing
        // else — the tty and pane are what the focus action needs.
        let title = line
            .split_once('"')
            .and_then(|(_, r)| r.split_once('"'))
            .map(|(t, _)| t.to_string())
            .unwrap_or_default();
        out.push(Surface { tty, title, pane: pane.clone() });
    }
    out
}

/// `haystack` after the first occurrence of `needle`.
fn after<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    haystack.find(needle).map(|i| &haystack[i + needle.len()..])
}

/// Every surface the host can describe. Empty for hosts that cannot be asked
/// cheaply — reading titles out of AppleScript means waiting on it, and that
/// can block for minutes.
pub fn surfaces(host: &Host) -> Vec<Surface> {
    match &host.kind {
        HostKind::Cmux { cli } => Command::new(cli)
            .args(["tree", "--all"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| parse_cmux_tree(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Bring the tab or pane on `tty` to the front.
///
/// Always spawned, never awaited. The AppleScript path in particular can hang
/// on an unanswered Automation permission prompt — measured at two minutes
/// before failing — and the dashboard must not be behind it.
pub fn focus(host: &Host, tty: Option<&str>, surfaces: &[Surface]) {
    let pane = tty.and_then(|t| surfaces.iter().find(|s| s.tty == t)).map(|s| s.pane.clone());
    match (&host.kind, pane, tty) {
        (HostKind::Cmux { cli }, Some(pane), _) => {
            let _ = Command::new(cli).args(["focus-pane", "--pane", &pane]).spawn();
        }
        (HostKind::AppleScript { app }, _, Some(tty)) => {
            let script = applescript_focus(app, tty);
            let _ = Command::new("osascript").args(["-e", &script]).spawn();
        }
        // Nothing precise available: bring the application forward. It does
        // not pick the tab, but with one window it is most of the value.
        _ => {
            let _ = Command::new("open").arg("-a").arg(&host.bundle).spawn();
        }
    }
}

/// Select the tab whose tty matches, then bring the app forward.
fn applescript_focus(app: &str, tty: &str) -> String {
    let dev = format!("/dev/{tty}");
    match app {
        "iTerm2" => format!(
            r#"tell application "iTerm2"
                 repeat with w in windows
                   repeat with t in tabs of w
                     repeat with s in sessions of t
                       if tty of s is "{dev}" then
                         select w
                         select t
                         activate
                         return
                       end if
                     end repeat
                   end repeat
                 end repeat
               end tell"#
        ),
        _ => format!(
            r#"tell application "Terminal"
                 repeat with w in windows
                   repeat with t in tabs of w
                     if tty of t is "{dev}" then
                       set selected of t to true
                       set index of w to 1
                       activate
                       return
                     end if
                   end repeat
                 end repeat
               end tell"#
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `cmux tree --all` output, trimmed. The box-drawing prefixes and
    /// the trailing `tty=` are exactly as the CLI emits them.
    const TREE: &str = r#"window window:1 [current] ◀ active
├── workspace workspace:1 "Group 1"
│   └── pane pane:1 [focused]
│       └── surface surface:1 [terminal] "…/Documents/Projects/axterio" [selected] tty=ttys011
├── workspace workspace:3 "Ownit Produkce [FE]"
│   ├── pane pane:3
│   │   └── surface surface:3 [terminal] "✳ Switch to dev branch" [selected] tty=ttys010
│   └── pane pane:4 [focused]
│       └── surface surface:4 [terminal] "pnpm storybook" [selected] tty=ttys012
"#;

    #[test]
    fn every_surface_is_read_with_its_tty_title_and_pane() {
        let s = parse_cmux_tree(TREE);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], Surface {
            tty: "ttys011".into(),
            title: "…/Documents/Projects/axterio".into(),
            pane: "pane:1".into(),
        });
    }

    #[test]
    fn a_surface_belongs_to_the_pane_above_it_not_the_first_one_seen() {
        // Two panes in one workspace: the second surface must carry pane:4,
        // which is the whole reason this parse is positional.
        let s = parse_cmux_tree(TREE);
        assert_eq!(s[1].pane, "pane:3", "first pane of the workspace");
        assert_eq!(s[2].pane, "pane:4", "second pane, not the first");
        assert_eq!(s[2].title, "pnpm storybook");
    }

    #[test]
    fn output_that_is_not_a_tree_yields_nothing_rather_than_guesses() {
        for junk in ["", "cmux: not running", "error: socket refused\n"] {
            assert!(parse_cmux_tree(junk).is_empty(), "{junk:?}");
        }
    }

    #[test]
    fn a_surface_with_no_tty_is_skipped() {
        // Browser surfaces have no controlling terminal, and nothing whirr
        // shows could be joined to one.
        let text = "│   └── pane pane:9 [focused]\n│       └── surface surface:9 [browser] \"docs\"\n";
        assert!(parse_cmux_tree(text).is_empty());
    }

    #[test]
    fn the_process_table_parses_into_parent_links() {
        let m = parse_parents("  80411  3012\n   3012  3010\n   3010   831\n");
        assert_eq!(m.get(&80411), Some(&3012));
        assert_eq!(m.get(&3010), Some(&831), "login's parent is the terminal app");
    }

    #[test]
    fn junk_lines_in_the_process_table_are_skipped() {
        let m = parse_parents("PID PPID\nnot numbers\n\n  7  1\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(&7), Some(&1));
    }

    #[test]
    fn the_walk_stops_rather_than_looping_on_a_cycle() {
        // Not hypothetical: a pid that is its own parent, or a pair that
        // point at each other, would spin forever without the bound.
        let m = HashMap::from([(10, 11), (11, 10)]);
        assert_eq!(detect_with(10, &m), None);
    }

    #[test]
    fn the_walk_gives_up_at_the_top_of_the_tree() {
        let m = HashMap::from([(10, 2), (2, 1)]);
        assert_eq!(detect_with(10, &m), None, "reaching launchd means no host was found");
    }

    #[test]
    fn a_bundle_is_found_from_a_binary_buried_inside_it() {
        assert_eq!(
            bundle_of(Path::new("/Applications/cmux.app/Contents/MacOS/cmux")),
            Some(PathBuf::from("/Applications/cmux.app"))
        );
        assert_eq!(
            bundle_of(Path::new("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal")),
            Some(PathBuf::from("/System/Applications/Utilities/Terminal.app"))
        );
    }

    #[test]
    fn a_binary_outside_any_bundle_has_no_host() {
        assert_eq!(bundle_of(Path::new("/usr/bin/login")), None);
        assert_eq!(bundle_of(Path::new("/opt/homebrew/bin/whirr")), None);
    }

    #[test]
    fn an_unrecognised_app_is_opaque_rather_than_guessed_at() {
        // Ghostty exposes no automation interface. Claiming otherwise would
        // send keystrokes into a void; Opaque still gets the activate rung.
        assert_eq!(classify(Path::new("/Applications/Ghostty.app")), HostKind::Opaque);
        assert_eq!(classify(Path::new("/Applications/Nonesuch.app")), HostKind::Opaque);
    }

    #[test]
    fn the_scriptable_terminals_are_recognised() {
        assert_eq!(
            classify(Path::new("/System/Applications/Utilities/Terminal.app")),
            HostKind::AppleScript { app: "Terminal" }
        );
        assert_eq!(
            classify(Path::new("/Applications/iTerm2.app")),
            HostKind::AppleScript { app: "iTerm2" }
        );
    }

    #[test]
    fn cmux_without_its_bundled_cli_falls_back_to_opaque() {
        // The CLI lives inside the bundle rather than on PATH. A cmux install
        // that does not have it is still a terminal whirr can bring forward.
        assert_eq!(classify(Path::new("/nonexistent/cmux.app")), HostKind::Opaque);
    }

    #[test]
    fn the_focus_script_targets_the_device_not_the_bare_tty_name() {
        // Terminal.app and iTerm2 both report `/dev/ttys004`, not `ttys004`.
        let s = applescript_focus("Terminal", "ttys004");
        assert!(s.contains("/dev/ttys004"), "{s}");
        assert!(!s.contains("is \"ttys004\""), "the bare name would never match");
    }
}
