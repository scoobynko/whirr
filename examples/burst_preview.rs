//! Animated size comparison for the burst fan, driving the real
//! `ui::burst::render`. Three candidate footprints side by side, with the
//! wordmark for scale, at idle spin then hot spin. Deleted once the size is
//! settled (plan Task 7).

use std::io::Write;
use std::time::{Duration, Instant};

use ratatui::style::Color;

const LOGO4: [&str; 4] = [
    "█   █ █  █ ▄█▄ █▀▀▄ █▀▀▄",
    "█   █ █▄▄█  █  █▄▄▀ █▄▄▀",
    "█ ▄ █ █  █  █  █ ▀▄ █ ▀▄",
    "▀▄▀▄▀ █  █ ▄█▄ █  █ █  █",
];
/// (cols, rows, label) — the header band is 9 rows; shorter bursts centre in it.
const SIZES: [(u16, u16, &str); 3] =
    [(21, 9, "A 21x9  r17 (now)"), (19, 7, "B 19x7  r13"), (15, 5, "C 15x5  r9")];
const BAND: usize = 9;
const COLD_DPS: f32 = 360.0 / 14.0;
const HOT_DPS: f32 = 360.0 / 2.0;
const FPS: u64 = 16;

/// One pane's 9 rows, already ANSI-coloured and padded to `w`.
fn pane(w: u16, h: u16, angle: f32) -> Vec<String> {
    let lines = whirr::ui::burst::render(w, h, angle);
    let top = (BAND - h as usize) / 2;
    (0..BAND)
        .map(|y| {
            if y < top || y >= top + h as usize {
                return " ".repeat(w as usize);
            }
            lines[y - top]
                .spans
                .iter()
                .map(|s| match s.style.fg {
                    Some(Color::Rgb(r, g, b)) => {
                        format!("\x1b[38;2;{r};{g};{b}m{}\x1b[0m", s.content)
                    }
                    _ => s.content.to_string(),
                })
                .collect()
        })
        .collect()
}

fn main() {
    let total = Duration::from_secs(24);
    let frame = Duration::from_millis(1000 / FPS);
    let start = Instant::now();
    let mut angle = 0.0f32;
    let mut out = std::io::stdout();

    while start.elapsed() < total {
        let elapsed = start.elapsed().as_secs_f32();
        let (dps, label) = if elapsed < 12.0 {
            (COLD_DPS, "idle  360°/14s")
        } else {
            (HOT_DPS, "HOT   360°/2s ")
        };
        angle = (angle + dps / FPS as f32).rem_euclid(360.0);

        let panes: Vec<Vec<String>> =
            SIZES.iter().map(|(w, h, _)| pane(*w, *h, angle)).collect();

        let mut buf = String::from("\x1b[H\x1b[2J\n");
        buf.push_str(&format!("  {:<26}", ""));
        for (w, _, name) in SIZES {
            buf.push_str(&format!("{name:<w$}   ", w = w as usize));
        }
        buf.push_str("\n\n");
        for y in 0..BAND {
            let logo = if (2..6).contains(&y) { LOGO4[y - 2] } else { "" };
            buf.push_str(&format!("  {logo:<26}"));
            for p in &panes {
                buf.push_str(&p[y]);
                buf.push_str("   ");
            }
            buf.push('\n');
        }
        buf.push_str(&format!(
            "\n  {label}    {:.0}s left\n",
            (total - start.elapsed()).as_secs_f32()
        ));
        let _ = out.write_all(buf.as_bytes());
        let _ = out.flush();
        std::thread::sleep(frame);
    }
    println!("\ndone — A 21x9 (current), B 19x7, C 15x5.");
}
