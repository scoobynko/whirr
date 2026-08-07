//! "A newer whirr exists" — whirr's only network call, and the only one it
//! will ever make on the dashboard's behalf.
//!
//! Everything else whirr reads is local IOKit/sysctl. That is a property worth
//! keeping deliberate, so this module is easy to switch off (`--no-update-check`),
//! never runs more than once a day, and never blocks anything: the check lives
//! on its own thread and the result arrives over the same channel the samplers
//! use.
//!
//! The fetch shells out to `curl` rather than linking an HTTP client. whirr
//! already shells out to `lsof` and `open`; adding a TLS stack and its
//! transitive tree to a five-dependency crate to make one GET a day would be
//! the largest thing in the binary by far.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::sampler::Snapshot;

/// How stale a cached answer may be before whirr asks again. Once a day is
/// far more often than whirr releases, and it means someone who restarts the
/// dashboard twenty times a day makes one request, not twenty.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// crates.io rejects requests without a User-Agent that identifies the caller
/// and offers a way to contact them. This is that.
const USER_AGENT: &str =
    concat!("whirr/", env!("CARGO_PKG_VERSION"), " (https://github.com/scoobynko/whirr)");

const ENDPOINT: &str = "https://crates.io/api/v1/crates/whirr";

/// A newer version than the one running, and how to get it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub latest: String,
    /// The command that upgrades *this* installation — see `upgrade_hint`.
    pub hint: &'static str,
}

/// Pull `max_stable_version` out of a crates.io crate response.
///
/// Hand-rolled rather than pulling in a JSON parser: this reads one string
/// field from one endpoint whose shape is fixed, and `sampler::slow` already
/// hand-parses lsof for the same reason. `max_stable_version` (not `newest`)
/// is deliberate — it skips pre-releases, which nobody should be nudged onto.
pub fn parse_latest(json: &str) -> Option<String> {
    let key = "\"max_stable_version\":";
    let rest = json.find(key).map(|i| &json[i + key.len()..])?;
    let rest = rest.trim_start();
    // A crate with no stable release at all reports null here.
    let inner = rest.strip_prefix('"')?;
    let end = inner.find('"')?;
    let version = &inner[..end];
    (!version.is_empty()).then(|| version.to_string())
}

/// Is `latest` a higher version than `current`?
///
/// Compares dot-separated numbers rather than strings, because "0.3.10" sorts
/// below "0.3.9" lexically and would silently stop reporting updates after the
/// tenth patch. Anything unparseable answers `false`: staying quiet is the
/// right failure for a notice nobody asked for.
pub fn is_newer(current: &str, latest: &str) -> bool {
    fn parts(v: &str) -> Option<Vec<u64>> {
        // Ignore any pre-release or build suffix; only the numeric core is
        // compared, and a pre-release is never offered as an upgrade anyway.
        v.split(['-', '+']).next()?.split('.').map(|p| p.parse().ok()).collect()
    }
    match (parts(current), parts(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// The upgrade command for the installation this binary belongs to.
///
/// Telling someone who ran `cargo install` to use `brew upgrade` is worse than
/// saying nothing, so an unrecognised location gets the neutral answer rather
/// than a confident guess.
pub fn upgrade_hint(exe: &Path) -> &'static str {
    let p = exe.to_string_lossy();
    if p.contains("/Cellar/") || p.contains("/homebrew/") || p.contains("/linuxbrew/") {
        "brew update && brew upgrade whirr"
    } else if p.contains("/.cargo/") {
        "cargo install whirr --force"
    } else {
        "see github.com/scoobynko/whirr/releases"
    }
}

/// Where the last answer is remembered, so restarting whirr doesn't re-ask.
fn cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("whirr").join("update-check"))
}

/// `<unix seconds>\n<version>` — two lines, so a corrupt or half-written file
/// is trivially rejected rather than misread.
fn encode_cache(checked_at: u64, latest: &str) -> String {
    format!("{checked_at}\n{latest}\n")
}

fn decode_cache(text: &str) -> Option<(u64, String)> {
    let mut lines = text.lines();
    let at: u64 = lines.next()?.trim().parse().ok()?;
    let version = lines.next()?.trim();
    (!version.is_empty()).then(|| (at, version.to_string()))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A cached answer, if one is present and still fresh.
fn cached_fresh(now: u64) -> Option<String> {
    let text = std::fs::read_to_string(cache_path()?).ok()?;
    let (at, version) = decode_cache(&text)?;
    // `now < at` means the clock moved backwards; treat that as stale rather
    // than trusting a timestamp from the future.
    (now >= at && now - at < MAX_AGE.as_secs()).then_some(version)
}

fn write_cache(now: u64, latest: &str) {
    let Some(path) = cache_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // A cache that cannot be written costs one request per launch, which is
    // not worth telling the user about.
    let _ = std::fs::write(path, encode_cache(now, latest));
}

/// One GET, with a hard timeout so a hung network cannot leave a thread
/// waiting forever.
fn fetch() -> Option<String> {
    let out = Command::new("curl")
        .args(["-sS", "--max-time", "5", "-A", USER_AGENT, ENDPOINT])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_latest(&String::from_utf8_lossy(&out.stdout))
}

/// Check once, in the background, and report only if there is something to
/// report.
///
/// Spawned rather than awaited: the first thing this does may be a DNS lookup
/// on a captive-portal network, and the dashboard must not wait for it.
pub fn spawn(tx: Sender<Snapshot>) {
    std::thread::spawn(move || {
        let now = now_secs();
        let latest = match cached_fresh(now) {
            Some(v) => v,
            None => {
                let v = fetch()?;
                write_cache(now, &v);
                v
            }
        };
        if !is_newer(env!("CARGO_PKG_VERSION"), &latest) {
            return None;
        }
        let hint = std::env::current_exe()
            .map(|p| upgrade_hint(&p))
            .unwrap_or("see github.com/scoobynko/whirr/releases");
        tx.send(Snapshot::Update(Update { latest, hint })).ok()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed to the fields this reads, in the shape crates.io returns them.
    const PAYLOAD: &str = r#"{"categories":[],"crate":{"id":"whirr","name":"whirr",
        "newest_version":"0.4.0-rc.1","max_stable_version":"0.3.6","description":"x"}}"#;

    #[test]
    fn the_latest_stable_version_is_read_from_a_crates_io_payload() {
        assert_eq!(parse_latest(PAYLOAD).as_deref(), Some("0.3.6"));
    }

    #[test]
    fn a_crate_with_no_stable_release_reports_nothing() {
        // crates.io sends null here when everything published is a
        // pre-release. Offering "null" as a version would be worse than
        // staying quiet.
        assert_eq!(parse_latest(r#"{"crate":{"max_stable_version":null}}"#), None);
        assert_eq!(parse_latest(r#"{"crate":{"max_stable_version":""}}"#), None);
    }

    #[test]
    fn a_response_that_is_not_the_expected_shape_reports_nothing() {
        assert_eq!(parse_latest(""), None);
        assert_eq!(parse_latest("<html>503 Service Unavailable</html>"), None);
        assert_eq!(parse_latest(r#"{"errors":[{"detail":"Not Found"}]}"#), None);
    }

    #[test]
    fn a_higher_version_is_newer_and_nothing_else_is() {
        assert!(is_newer("0.3.5", "0.3.6"));
        assert!(is_newer("0.3.5", "0.4.0"));
        assert!(is_newer("0.3.5", "1.0.0"));
        assert!(!is_newer("0.3.5", "0.3.5"));
        assert!(!is_newer("0.3.5", "0.3.4"));
        assert!(!is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn versions_are_compared_as_numbers_not_strings() {
        // The one that matters: "0.3.10" < "0.3.9" lexically, so a string
        // compare would go quiet forever after the tenth patch release.
        assert!(is_newer("0.3.9", "0.3.10"));
        assert!(!is_newer("0.3.10", "0.3.9"));
    }

    #[test]
    fn an_unparseable_version_never_claims_an_update() {
        assert!(!is_newer("0.3.5", "not-a-version"));
        assert!(!is_newer("garbage", "0.3.6"));
        assert!(!is_newer("0.3.5", ""));
    }

    #[test]
    fn the_upgrade_hint_matches_how_this_copy_was_installed() {
        let brew = upgrade_hint(Path::new("/opt/homebrew/Cellar/whirr/0.3.5/bin/whirr"));
        assert!(brew.contains("brew"), "homebrew install should get a brew command");
        let linked = upgrade_hint(Path::new("/opt/homebrew/bin/whirr"));
        assert!(linked.contains("brew"), "the symlinked path is a homebrew install too");
        let cargo = upgrade_hint(Path::new("/Users/me/.cargo/bin/whirr"));
        assert!(cargo.contains("cargo install"), "cargo install should get a cargo command");
    }

    #[test]
    fn an_unrecognised_location_gets_a_neutral_hint() {
        // Telling a cargo user to run brew upgrade is worse than saying
        // nothing, so an unknown path must not guess either way.
        let hint = upgrade_hint(Path::new("/usr/local/bin/whirr"));
        assert!(!hint.contains("brew"), "must not guess homebrew: {hint}");
        assert!(!hint.contains("cargo"), "must not guess cargo: {hint}");
        assert!(hint.contains("releases"), "should point somewhere useful: {hint}");
    }

    #[test]
    fn a_cache_entry_survives_a_round_trip() {
        let text = encode_cache(1_700_000_000, "0.3.6");
        assert_eq!(decode_cache(&text), Some((1_700_000_000, "0.3.6".to_string())));
    }

    #[test]
    fn a_damaged_cache_is_rejected_rather_than_misread() {
        assert_eq!(decode_cache(""), None);
        assert_eq!(decode_cache("not-a-timestamp\n0.3.6\n"), None);
        // Half-written: timestamp landed, version didn't.
        assert_eq!(decode_cache("1700000000\n"), None);
        assert_eq!(decode_cache("1700000000\n\n"), None);
    }
}
