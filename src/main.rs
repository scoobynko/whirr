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
        --list-sensors
                     Dump the HID temperature sensors and IOReport Energy
                     Model channels this Mac exposes, then exit

KEYS (while running):
    ↑ ↓        move the selection within the focused card
    tab        cycle focus between cards
    c / m      sort processes by CPU / memory
    k          kill the selected process or dev server (asks first)
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
    /// Run the dashboard. `no_fan` suppresses the header animation.
    Run { no_fan: bool },
}

fn parse_args(args: impl Iterator<Item = String>) -> Mode {
    let (mut no_fan, mut list_sensors) = (false, false);
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
            // Unknown arguments are ignored rather than fatal: this is a
            // dashboard with four flags, not a CLI with a grammar to enforce.
            _ => {}
        }
    }
    if list_sensors {
        Mode::ListSensors
    } else {
        Mode::Run { no_fan }
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

    let no_fan = match parse_args(std::env::args().skip(1)) {
        Mode::Print(text) => {
            println!("{text}");
            return Ok(());
        }
        Mode::ListSensors => {
            list_sensors();
            return Ok(());
        }
        Mode::Run { no_fan } => no_fan,
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
    sampler::spawn_samplers(tx);

    let mut app = App::new(no_fan);
    let mut last_fan = std::time::Instant::now();

    loop {
        let timeout = if app.no_fan {
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
                    app.on_key(key)
                }
                Event::Resize(_, _) => app.dirty = true,
                _ => {}
            }
        }
        while let Ok(snap) = rx.try_recv() {
            app.ingest(snap);
        }
        if !app.no_fan && last_fan.elapsed() >= app.fan_interval() {
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
        assert!(matches!(parse(&[]), Mode::Run { no_fan: false }));
    }

    #[test]
    fn no_fan_is_recognised_in_both_positions() {
        assert!(matches!(parse(&["--no-fan"]), Mode::Run { no_fan: true }));
        // Unknown arguments are ignored rather than fatal, so a flag after one
        // must still be seen.
        assert!(matches!(parse(&["--wat", "--no-fan"]), Mode::Run { no_fan: true }));
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
