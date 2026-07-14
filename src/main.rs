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
    // Temporary verification hook for Task 7 (removed in Task 8 once
    // --list-sensors supersedes it). Must run before terminal setup so it
    // works without a TTY.
    if std::env::args().any(|a| a == "--power-test") {
        let mut p = crate::mac::ioreport::PowerSampler::new().expect("ioreport");
        p.sample();
        std::thread::sleep(std::time::Duration::from_secs(2));
        println!("{:?}", p.sample().map(|s| (s.cpu_w, s.gpu_w, s.ane_w)));
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
