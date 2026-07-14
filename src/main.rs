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

fn main() -> io::Result<()> {
    // Diagnostic hook: dump HID temperature sensors and IOReport Energy
    // Model channels. Must run before terminal setup so it works without a
    // TTY.
    if std::env::args().any(|a| a == "--list-sensors") {
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
        return Ok(());
    }

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

    let no_fan = std::env::args().any(|a| a == "--no-fan");
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
            app.tick_fan();
            last_fan = std::time::Instant::now();
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
