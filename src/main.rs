use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

#[allow(dead_code)]
mod units;

#[allow(dead_code)]
mod history;

#[allow(dead_code)]
mod mac;

mod sampler;

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
    let mut latest_cpu: f32 = 0.0;

    loop {
        while let Ok(snap) = rx.try_recv() {
            if let sampler::Snapshot::Fast(f) = snap {
                latest_cpu = f.total_cpu;
            }
        }

        terminal.draw(|f| {
            f.render_widget(Paragraph::new(format!("cpu {latest_cpu:.0}%")), f.area());
        })?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                let ctrl_c =
                    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
                if key.code == KeyCode::Char('q') || ctrl_c {
                    break;
                }
            }
        }
    }
    Ok(())
}
