//! Animated gate for the burst fan: counter-rotating inner and outer ray
//! halves, braille sub-cell hairlines, no dash pulse. Two panes differing only
//! in how fast the outer ring counter-rotates against the inner one.
//!
//! Self-contained on purpose: `ui::burst` does not exist yet, and whichever
//! variant wins here becomes its reference implementation. Deleted once the
//! fan lands.

use std::io::Write;
use std::time::{Duration, Instant};

const COLS: usize = 21;
const ROWS: usize = 9;

// Geometry (spec §2), now split into two counter-rotating rings.
const RAYS: f32 = 10.0;
const RAY_STEP: f32 = 360.0 / RAYS;
const RAY_OFFSET: f32 = 18.0;
const HUB: f32 = 0.26;
/// Inner ring spans HUB..INNER_END, outer ring OUTER_START..1.0. The gap
/// between them is what makes the split read as two halves of one blade.
const INNER_END: f32 = 0.58;
const OUTER_START: f32 = 0.68;

// Motion (spec §6). Thermal spin, no wall-clock beat any more.
const COLD_DPS: f32 = 360.0 / 14.0;
const HOT_DPS: f32 = 360.0 / 2.0;
const FPS: u64 = 16;

// Brand colours (src/ui/theme.rs).
const TEXT: (u8, u8, u8) = (205, 214, 217);
const ACCENT: (u8, u8, u8) = (45, 225, 194);
const BG_CELL: (u8, u8, u8) = (18, 32, 36);
const MIN_BRIGHT: f32 = 0.5;

const THICK_DOTS: f32 = 0.75;
const SS: i32 = 3;
const DOT_ON: f32 = 0.4;
const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// Is `(x, y)` on a ray? Returns its perpendicular distance from the ray's
/// centre line and the ray's index. The ring the point falls in decides which
/// of the two angles applies, so the halves shear past each other.
fn ray_at(x: f32, y: f32, rad: f32, inner: f32, outer: f32) -> Option<(f32, usize)> {
    let r = x.hypot(y);
    let f = r / rad;
    let angle = if (HUB..=INNER_END).contains(&f) {
        inner
    } else if (OUTER_START..=1.0).contains(&f) {
        outer
    } else {
        return None;
    };
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

fn render(inner: f32, outer: f32) -> Vec<String> {
    let (dw, dh) = (COLS * 2, ROWS * 4);
    let (cx, cy) = ((dw as f32 - 1.0) / 2.0, (dh as f32 - 1.0) / 2.0);
    let rad = (dw.min(dh) as f32) / 2.0 - 1.0;
    let step = 1.0 / SS as f32;

    let mut cov = vec![vec![(0.0f32, 0usize); dw]; dh];
    for (j, row) in cov.iter_mut().enumerate() {
        for (i, slot) in row.iter_mut().enumerate() {
            let (mut hits, mut ray) = (0, 0);
            for a in 0..SS {
                for b in 0..SS {
                    let x = i as f32 + (a as f32 + 0.5) * step - 0.5 - cx;
                    let y = j as f32 + (b as f32 + 0.5) * step - 0.5 - cy;
                    if let Some((d, k)) = ray_at(x, y, rad, inner, outer) {
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

/// How fast the outer ring counter-rotates, as a fraction of the inner ring's
/// speed. Negative = opposite direction.
const RATIO_A: f32 = -1.0;
const RATIO_B: f32 = -0.4;

fn main() {
    let total = Duration::from_secs(24);
    let frame = Duration::from_millis(1000 / FPS);
    let start = Instant::now();
    let (mut a_in, mut a_out) = (0.0f32, 0.0f32);
    let (mut b_in, mut b_out) = (0.0f32, 0.0f32);
    let mut out = std::io::stdout();

    while start.elapsed() < total {
        let elapsed = start.elapsed().as_secs_f32();
        let (dps, label) = if elapsed < 12.0 {
            (COLD_DPS, "idle  360°/14s")
        } else {
            (HOT_DPS, "HOT   360°/2s ")
        };
        let d = dps / FPS as f32;
        a_in = (a_in + d).rem_euclid(360.0);
        a_out = (a_out + d * RATIO_A).rem_euclid(360.0);
        b_in = (b_in + d).rem_euclid(360.0);
        b_out = (b_out + d * RATIO_B).rem_euclid(360.0);

        let a = render(a_in, a_out);
        let b = render(b_in, b_out);

        let mut buf = String::from("\x1b[H\x1b[2J\n");
        buf.push_str("   A: outer -1.0x               B: outer -0.4x\n\n");
        for (al, bl) in a.iter().zip(b.iter()) {
            buf.push_str(&format!("   {al}      {bl}\n"));
        }
        buf.push_str(&format!(
            "\n   {label}    {:.0}s left\n",
            (total - start.elapsed()).as_secs_f32()
        ));
        let _ = out.write_all(buf.as_bytes());
        let _ = out.flush();

        std::thread::sleep(frame);
    }
    println!("\ndone — A: equal and opposite.  B: outer slower.");
}
