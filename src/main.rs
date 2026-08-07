use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;

use whirr::app::App;
use whirr::{mac, sampler, ui};

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

const HELP: &str = "\
whirr — a macOS system dashboard that lives in your terminal

USAGE:
    whirr [OPTIONS]

OPTIONS:
    -h, --help       Print this help and exit
    -V, --version    Print the version and exit
        --no-fan     Don't animate the burst fan in the header
        --no-update-check
                     Don't check crates.io for a newer release. This is the
                     only network request whirr makes; it happens at most
                     once a day and never blocks the dashboard
        --list-sensors
                     Dump the HID temperature sensors and IOReport Energy
                     Model channels this Mac exposes, then exit

KEYS (while running):
    ↑ ↓        move the selection within the focused card
    tab        cycle focus between cards
    c / m      sort processes by CPU / memory
    o          localhost: open the dev server in your browser (asks which
               port when the row offers more than one)
               sessions:  jump to the terminal the session is running in
    k          kill the selected process or dev server (a dialog asks first)
    s          settings: theme, accent colour, background, fan
    q          quit
";

/// What the command line asked for. Parsed up front so every mode is visible
/// in one place, rather than each one re-scanning `args()` wherever it happens
/// to be needed.
enum Mode {
    /// Print `text` to stdout and exit successfully.
    Print(String),
    /// Dump sensor diagnostics and exit.
    ListSensors,
    /// Run the dashboard. `no_fan` suppresses the header animation;
    /// `no_update_check` suppresses whirr's only network call.
    Run { no_fan: bool, no_update_check: bool },
}

fn parse_args(args: impl Iterator<Item = String>) -> Mode {
    let (mut no_fan, mut list_sensors, mut no_update_check) = (false, false, false);
    for arg in args {
        match arg.as_str() {
            // --help and --version outrank every other mode: a user asking
            // what this thing is should never land in an alternate screen they
            // then have to escape from, whatever else is on the line. They can
            // return early because nothing later outranks them; --list-sensors
            // cannot, or `--list-sensors --help` would dump sensors instead.
            "-h" | "--help" => return Mode::Print(HELP.to_string()),
            "-V" | "--version" => {
                return Mode::Print(format!("whirr {}", env!("CARGO_PKG_VERSION")))
            }
            "--list-sensors" => list_sensors = true,
            "--no-fan" => no_fan = true,
            "--no-update-check" => no_update_check = true,
            // Unknown arguments are ignored rather than fatal: this is a
            // dashboard with four flags, not a CLI with a grammar to enforce.
            _ => {}
        }
    }
    if list_sensors {
        Mode::ListSensors
    } else {
        Mode::Run { no_fan, no_update_check }
    }
}

/// Dump the temperature sensors and power channels this Mac exposes. Runs
/// before any terminal setup so it works without a TTY.
fn list_sensors() {
    match mac::hid_temp::TempSensor::new() {
        Some(t) => {
            for (name, v) in t.list() {
                println!("{name:32} {v:6.1} °C");
            }
        }
        None => println!("no HID temperature client"),
    }
    if let Some(p) = mac::ioreport::PowerSampler::new() {
        println!("--- IOReport Energy Model channels ---");
        for name in p.channel_names() {
            println!("{name}");
        }
    }
}

fn main() -> io::Result<()> {
    // Die quietly on a closed pipe (`whirr --list-sensors | head`) instead of
    // panicking — restore the default SIGPIPE disposition Rust overrides.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let (no_fan, no_update_check) = match parse_args(std::env::args().skip(1)) {
        Mode::Print(text) => {
            println!("{text}");
            return Ok(());
        }
        Mode::ListSensors => {
            list_sensors();
            return Ok(());
        }
        Mode::Run { no_fan, no_update_check } => (no_fan, no_update_check),
    };

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let _guard = TerminalGuard;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let (tx, rx) = std::sync::mpsc::channel();
    if !no_update_check {
        // Spawned, never awaited: the first thing it may do is a DNS lookup on
        // a captive-portal network, and the dashboard must not wait for it.
        whirr::update::spawn(tx.clone());
    }
    // Kept for the background actions below: the samplers take a clone, not
    // the original, so the event loop can still send on it.
    sampler::spawn_samplers(tx.clone());

    let mut app = App::new(no_fan);
    app.load_settings(no_fan);
    let mut last_fan = std::time::Instant::now();

    loop {
        let timeout = if app.no_fan() {
            Duration::from_millis(250)
        } else {
            app.fan_interval()
                .checked_sub(last_fan.elapsed())
                .unwrap_or(Duration::ZERO)
                .max(Duration::from_millis(30))
        };
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind != crossterm::event::KeyEventKind::Release => {
                    app.on_key(key);
                    // `App` decides what to open; the spawning lives here so
                    // no keypress in a test can launch a browser. Errors are
                    // dropped: `open` failing is not worth stealing a row of
                    // the dashboard to report.
                    if let Some(url) = app.take_open_request() {
                        let _ = std::process::Command::new("open").arg(url).spawn();
                    }
                    // Written here rather than in `App` so no test can
                    // rewrite the real config just by pressing keys.
                    if let Some(settings) = app.take_settings_save() {
                        settings.save();
                    }
                    // Off the render loop entirely: finding the host reads the
                    // whole process table, and the AppleScript path can hang
                    // for minutes on an unanswered permission prompt.
                    if let Some((pid, tty)) = app.take_focus_request() {
                        let tx = tx.clone();
                        std::thread::spawn(move || {
                            // Anything other than a clean jump is reported.
                            // The fallback rung activates an app the user is
                            // often already inside, which is indistinguishable
                            // from nothing happening unless whirr says so.
                            let problem = match whirr::host::detect(pid) {
                                Some(host) => {
                                    let surfaces = whirr::host::surfaces(&host);
                                    whirr::host::focus(&host, tty.as_deref(), &surfaces).err()
                                }
                                None => Some("couldn't tell which terminal that session is in".into()),
                            };
                            if let Some(text) = problem {
                                let _ = tx.send(whirr::sampler::Snapshot::Notice(text));
                            }
                        });
                    }
                }
                Event::Resize(_, _) => app.dirty = true,
                _ => {}
            }
        }
        while let Ok(snap) = rx.try_recv() {
            app.ingest(snap);
        }
        if !app.no_fan() && last_fan.elapsed() >= app.fan_interval() {
            let now = std::time::Instant::now();
            app.tick_fan(now - last_fan);
            last_fan = now;
        }
        if app.should_quit {
            break;
        }
        if app.dirty {
            terminal.draw(|f| ui::draw(f, &app))?;
            app.dirty = false;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_args, Mode, HELP};

    fn parse(args: &[&str]) -> Mode {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_arguments_runs_the_dashboard_with_the_fan() {
        assert!(matches!(parse(&[]), Mode::Run { no_fan: false, .. }));
    }

    #[test]
    fn no_fan_is_recognised_in_both_positions() {
        assert!(matches!(parse(&["--no-fan"]), Mode::Run { no_fan: true, .. }));
        // Unknown arguments are ignored rather than fatal, so a flag after one
        // must still be seen.
        assert!(matches!(parse(&["--wat", "--no-fan"]), Mode::Run { no_fan: true, .. }));
    }

    #[test]
    fn the_update_check_is_on_unless_it_is_turned_off() {
        assert!(matches!(parse(&[]), Mode::Run { no_update_check: false, .. }));
        assert!(
            matches!(parse(&["--no-update-check"]), Mode::Run { no_update_check: true, .. }),
            "the one network call whirr makes has to be refusable"
        );
    }

    #[test]
    fn the_two_run_flags_are_independent() {
        let m = parse(&["--no-fan", "--no-update-check"]);
        assert!(matches!(m, Mode::Run { no_fan: true, no_update_check: true }));
    }

    #[test]
    fn version_reports_the_crate_version() {
        // The Homebrew formula's `test do` block runs `whirr --version`, so
        // this string is load-bearing for the release pipeline: it must be
        // non-empty, carry the binary's name, and match Cargo.toml exactly.
        let Mode::Print(text) = parse(&["--version"]) else {
            panic!("--version must print and exit");
        };
        assert_eq!(text, format!("whirr {}", env!("CARGO_PKG_VERSION")));
        assert!(text.len() > "whirr ".len(), "version string is empty: {text:?}");
        assert!(matches!(parse(&["-V"]), Mode::Print(_)), "-V is the short form");
    }

    #[test]
    fn help_lists_every_flag_the_parser_accepts() {
        let Mode::Print(text) = parse(&["--help"]) else {
            panic!("--help must print and exit");
        };
        assert_eq!(text, HELP);
        for flag in ["--help", "--version", "--no-fan", "--list-sensors"] {
            assert!(text.contains(flag), "help text never mentions {flag}");
        }
        assert!(matches!(parse(&["-h"]), Mode::Print(_)), "-h is the short form");
    }

    #[test]
    fn help_and_version_win_over_the_running_modes() {
        // Whatever else is on the line, a user asking what this thing is must
        // never end up in an alternate screen they then have to escape from.
        assert!(matches!(parse(&["--no-fan", "--help"]), Mode::Print(_)));
        assert!(matches!(parse(&["--list-sensors", "--help"]), Mode::Print(_)));
        assert!(matches!(parse(&["--no-fan", "--version"]), Mode::Print(_)));
    }

    #[test]
    fn list_sensors_wins_over_running_but_not_over_help() {
        assert!(matches!(parse(&["--list-sensors"]), Mode::ListSensors));
        assert!(matches!(parse(&["--no-fan", "--list-sensors"]), Mode::ListSensors));
        assert!(matches!(parse(&["--help", "--list-sensors"]), Mode::Print(_)));
    }
}
