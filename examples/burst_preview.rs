//! Animated side-by-side gate for the burst fan: braille sub-cell hairlines
//! against directional stroke glyphs — same geometry, same motion, running at
//! the real frame rate. See
//! `docs/superpowers/specs/2026-07-27-whirr-burst-fan-design.md` §9.
//!
//! Self-contained on purpose: `ui::burst` does not exist yet, and whichever
//! renderer wins here becomes its reference implementation. Deleted once the
//! fan lands.

use std::io::Write;
use std::time::{Duration, Instant};

const COLS: usize = 21;
const ROWS: usize = 9;

// Geometry, identical for both renderers (spec §2).
const RAYS: f32 = 10.0;
const RAY_STEP: f32 = 360.0 / RAYS;
const RAY_OFFSET: f32 = 18.0;
const HUB: f32 = 0.26;
const DASH_L: f32 = 0.74;

// Motion (spec §6): the beat is wall-clock, the spin is thermal.
const BEAT_SECS: f32 = 2.0;
const COLD_DPS: f32 = 360.0 / 14.0;
const HOT_DPS: f32 = 360.0 / 2.0;
const FPS: u64 = 16;

// Brand colours (src/ui/theme.rs).
const TEXT: (u8, u8, u8) = (205, 214, 217);
const ACCENT: (u8, u8, u8) = (45, 225, 194);
const BG_CELL: (u8, u8, u8) = (18, 32, 36);
const MIN_BRIGHT: f32 = 0.5;

/// Dash duty differs per renderer: braille resolves a 0.14-of-radius gap, but
/// at cell resolution that is under one cell, so the strokes need a wider gap
/// for the beat to read at all.
const DUTY_BRAILLE: f32 = 0.86;
const DUTY_STROKE: f32 = 0.78;

/// Is `(x, y)` on a ray? Returns its perpendicular distance from the ray's
/// centre line and the ray's index. `x`/`y` are offsets from the centre in the
/// renderer's own units; `rad` is the radius in those same units.
fn ray_at(x: f32, y: f32, rad: f32, angle: f32, phase: f32, duty: f32) -> Option<(f32, usize)> {
    let r = x.hypot(y);
    let f = r / rad;
    if !(HUB..=1.0).contains(&f) {
        return None;
    }
    if ((f - HUB - phase) / DASH_L).rem_euclid(1.0) >= duty {
        return None;
    }
    let a = y.atan2(x).to_degrees() - angle;
    let k = ((a - RAY_OFFSET) / RAY_STEP).round();
    let dperp = (a - (RAY_OFFSET + RAY_STEP * k)).to_radians().sin().abs() * r;
    Some((dperp, (k as i32).rem_euclid(RAYS as i32) as usize))
}

fn paint(ch: char, tone: (u8, u8, u8), bright: f32) -> String {
    let t = bright.clamp(MIN_BRIGHT, 1.0);
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t) as u8;
    let (r, g, b) = (mix(BG_CELL.0, tone.0), mix(BG_CELL.1, tone.1), mix(BG_CELL.2, tone.2));
    format!("\x1b[38;2;{r};{g};{b}m{ch}\x1b[0m")
}

fn tone_for(ray: usize) -> (u8, u8, u8) {
    if ray.is_multiple_of(2) {
        TEXT
    } else {
        ACCENT
    }
}

const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
const THICK_DOTS: f32 = 0.75;
const SS: i32 = 3;
const DOT_ON: f32 = 0.4;

fn render_braille(angle: f32, beat: f32) -> Vec<String> {
    let (dw, dh) = (COLS * 2, ROWS * 4);
    let (cx, cy) = ((dw as f32 - 1.0) / 2.0, (dh as f32 - 1.0) / 2.0);
    let rad = (dw.min(dh) as f32) / 2.0 - 1.0;
    let phase = beat * DASH_L;
    let step = 1.0 / SS as f32;

    let mut cov = vec![vec![(0.0f32, 0usize); dw]; dh];
    for (j, row) in cov.iter_mut().enumerate() {
        for (i, slot) in row.iter_mut().enumerate() {
            let (mut hits, mut ray) = (0, 0);
            for a in 0..SS {
                for b in 0..SS {
                    let x = i as f32 + (a as f32 + 0.5) * step - 0.5 - cx;
                    let y = j as f32 + (b as f32 + 0.5) * step - 0.5 - cy;
                    if let Some((d, k)) = ray_at(x, y, rad, angle, phase, DUTY_BRAILLE) {
                        if d <= THICK_DOTS {
                            hits += 1;
                            ray = k;
                        }
                    }
                }
            }
            *slot = (hits as f32 / (SS * SS) as f32, ray);
        }
    }

    (0..ROWS)
        .map(|cy| {
            (0..COLS)
                .map(|cx| {
                    let (mut bits, mut sum, mut lit) = (0u8, 0.0, 0);
                    let mut votes = [0u32; 2];
                    for (dy, drow) in DOTS.iter().enumerate() {
                        for (dx, bit) in drow.iter().enumerate() {
                            let (c, k) = cov[cy * 4 + dy][cx * 2 + dx];
                            if c >= DOT_ON {
                                bits |= bit;
                                sum += c;
                                lit += 1;
                                votes[k % 2] += 1;
                            }
                        }
                    }
                    if lit == 0 {
                        return " ".to_string();
                    }
                    let tone = if votes[0] >= votes[1] { TEXT } else { ACCENT };
                    let ch = char::from_u32(0x2800 + u32::from(bits)).unwrap();
                    paint(ch, tone, sum / lit as f32)
                })
                .collect()
        })
        .collect()
}

const ASPECT: f32 = 2.0;
const THICK_CELLS: f32 = 0.55;

fn render_strokes(angle: f32, beat: f32) -> Vec<String> {
    let (cx, cy) = ((COLS as f32 - 1.0) / 2.0, (ROWS as f32 - 1.0) / 2.0);
    // Radius in cell-width units — rows are twice as tall as they are wide.
    let rad = (COLS as f32 / 2.0).min(ROWS as f32 * ASPECT / 2.0) - 0.5;
    let phase = beat * DASH_L;

    (0..ROWS)
        .map(|row| {
            (0..COLS)
                .map(|col| {
                    let x = col as f32 - cx;
                    let y = (row as f32 - cy) * ASPECT;
                    match ray_at(x, y, rad, angle, phase, DUTY_STROKE) {
                        Some((d, k)) if d <= THICK_CELLS => {
                            // The on-screen angle picks the glyph; how centred
                            // the ray sits in the cell picks the brightness.
                            let sa = y.atan2(x).to_degrees().rem_euclid(180.0);
                            let ch = if !(20.0..160.0).contains(&sa) {
                                '─'
                            } else if sa < 70.0 {
                                '╲'
                            } else if sa < 110.0 {
                                '│'
                            } else {
                                '╱'
                            };
                            paint(ch, tone_for(k), 1.0 - 0.5 * (d / THICK_CELLS))
                        }
                        _ => " ".to_string(),
                    }
                })
                .collect()
        })
        .collect()
}

fn main() {
    let total = Duration::from_secs(20);
    let frame = Duration::from_millis(1000 / FPS);
    let start = Instant::now();
    let mut angle = 0.0f32;
    let mut out = std::io::stdout();

    while start.elapsed() < total {
        let elapsed = start.elapsed().as_secs_f32();
        let (dps, label) = if elapsed < 8.0 {
            (COLD_DPS, "idle  360°/14s")
        } else {
            (HOT_DPS, "HOT   360°/2s ")
        };
        angle = (angle + dps / FPS as f32).rem_euclid(360.0);
        let beat = (elapsed / BEAT_SECS).fract();

        let b = render_braille(angle, beat);
        let s = render_strokes(angle, beat);

        let mut buf = String::from("\x1b[H\x1b[2J\n");
        buf.push_str("   braille                      stroke glyphs\n\n");
        for (bl, sl) in b.iter().zip(s.iter()) {
            buf.push_str(&format!("   {bl}      {sl}\n"));
        }
        buf.push_str(&format!("\n   {label}    beat {beat:.2}    {:.0}s left\n",
            (total - start.elapsed()).as_secs_f32()));
        let _ = out.write_all(buf.as_bytes());
        let _ = out.flush();

        std::thread::sleep(frame);
    }
    println!("\ndone — braille left, stroke glyphs right.");
}
