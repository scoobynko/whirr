//! Radial burst for the header, rasterized onto a braille dot canvas.
//!
//! Geometry ported from a reference recording measured frame by frame: 10
//! hairline rays at 36° spacing (vertical rays top and bottom, no horizontal
//! ray) around an empty hub. The motion is not from the reference — each ray
//! is split into an inner and an outer segment that rotate in opposite
//! directions, and where the two line up the blade briefly reads continuous
//! from hub to rim. See
//! `docs/superpowers/specs/2026-07-27-whirr-burst-fan-design.md`.
//!
//! Braille dots are roughly square, so unlike the old cell-grid fan this needs
//! no aspect correction — the burst rasterizes as a true circle.

use ratatui::prelude::*;

use super::theme;

/// Rays per ring, and the angular spacing that follows.
const RAYS: i32 = 10;
const RAY_STEP: f32 = 360.0 / RAYS as f32;
/// First ray offset: puts rays at ±18°, ±54°, ±90°, ±126°, ±162° — vertical
/// rays top and bottom, no horizontal ray, exactly as measured.
const RAY_OFFSET: f32 = 18.0;

/// No ink inside this fraction of the radius.
pub const HUB: f32 = 0.26;
/// The inner ring spans `HUB..=INNER_END` and turns with `+angle`; the outer
/// spans `OUTER_START..=1.0` and turns with `-angle`. The band between them is
/// never lit — that gap is what makes the split read as two halves of one
/// blade rather than a smear.
pub const INNER_END: f32 = 0.58;
pub const OUTER_START: f32 = 0.68;

/// Ray half-thickness in dots — hairlines, one dot wide at every radius.
const THICK: f32 = 0.75;
/// Supersampling per dot, per axis.
const SS: i32 = 3;
/// A dot lights at this coverage.
const DOT_ON: f32 = 0.4;
/// Cells never dim below this, so faint ones survive a light terminal.
const MIN_BRIGHT: f32 = 0.5;

/// Per-dot `(coverage, ray index)` for one frame. `angle_deg` is the inner
/// ring's rotation; the outer ring uses its negation.
pub fn coverage(w: u16, h: u16, angle_deg: f32) -> Vec<Vec<(f32, usize)>> {
    let (dw, dh) = (w as usize * 2, h as usize * 4);
    let (cx, cy) = ((dw as f32 - 1.0) / 2.0, (dh as f32 - 1.0) / 2.0);
    let rad = (dw.min(dh) as f32) / 2.0 - 1.0;
    let step = 1.0 / SS as f32;
    (0..dh)
        .map(|j| {
            (0..dw)
                .map(|i| {
                    let (mut hits, mut ray) = (0, 0);
                    for a in 0..SS {
                        for b in 0..SS {
                            let x = i as f32 + (a as f32 + 0.5) * step - 0.5 - cx;
                            let y = j as f32 + (b as f32 + 0.5) * step - 0.5 - cy;
                            if let Some(k) = sample(x, y, rad, angle_deg) {
                                hits += 1;
                                ray = k;
                            }
                        }
                    }
                    (hits as f32 / (SS * SS) as f32, ray)
                })
                .collect()
        })
        .collect()
}

/// Is this point on a ray, and if so which one? `x`/`y` are dot offsets from
/// the centre. The ring the point falls in decides which rotation applies, so
/// the two halves shear past each other. The ray index is taken in that ring's
/// rotating frame, so a ray keeps its index — and therefore its tone — as it
/// turns.
fn sample(x: f32, y: f32, rad: f32, angle_deg: f32) -> Option<usize> {
    let f = x.hypot(y) / rad;
    let angle = if (HUB..=INNER_END).contains(&f) {
        angle_deg
    } else if (OUTER_START..=1.0).contains(&f) {
        -angle_deg
    } else {
        return None;
    };
    let a = y.atan2(x).to_degrees() - angle;
    let k = ((a - RAY_OFFSET) / RAY_STEP).round();
    // Perpendicular distance from the ray's centre line, in dots.
    let dperp = (a - (RAY_OFFSET + RAY_STEP * k)).to_radians().sin().abs() * x.hypot(y);
    if dperp > THICK {
        return None;
    }
    Some((k as i32).rem_euclid(RAYS) as usize)
}

/// Braille dot bit for each `(row, col)` within a cell.
const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// One frame, as `h` lines of `w` spans. Empty cells are plain spaces.
pub fn render(w: u16, h: u16, angle_deg: f32) -> Vec<Line<'static>> {
    let g = coverage(w, h, angle_deg);
    (0..h as usize)
        .map(|cy| {
            let spans: Vec<Span> = (0..w as usize)
                .map(|cx| {
                    let (mut bits, mut sum, mut lit) = (0u8, 0.0, 0);
                    // Ray parity wins the cell by lit-dot count: a cell holds
                    // one foreground, and just outside the hub two rays can
                    // share one.
                    let mut votes = [0u32; 2];
                    for (dy, row) in DOTS.iter().enumerate() {
                        for (dx, bit) in row.iter().enumerate() {
                            let (c, k) = g[cy * 4 + dy][cx * 2 + dx];
                            if c >= DOT_ON {
                                bits |= bit;
                                sum += c;
                                lit += 1;
                                votes[k % 2] += 1;
                            }
                        }
                    }
                    if lit == 0 {
                        return Span::raw(" ");
                    }
                    let tone = if votes[0] >= votes[1] { theme::TEXT } else { theme::ACCENT };
                    let bright = (sum / lit as f32).clamp(MIN_BRIGHT, 1.0);
                    let ch = char::from_u32(0x2800 + u32::from(bits)).unwrap();
                    Span::styled(
                        ch.to_string(),
                        Style::default().fg(theme::blend(theme::BG_CELL, tone, bright)),
                    )
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ui::theme;

    const W: u16 = 21;
    const H: u16 = 9;
    /// A radius inside the inner ring, and one inside the outer ring.
    const F_IN: f32 = 0.42;
    const F_OUT: f32 = 0.84;

    fn ink(angle: f32) -> usize {
        coverage(W, H, angle).iter().flatten().filter(|(c, _)| *c > 0.0).count()
    }

    /// Coverage at the dot nearest an absolute polar position. `abs_deg` is a
    /// screen angle in the same convention the rasterizer uses (atan2 with y
    /// growing downward), NOT a ring-relative one.
    fn probe(angle: f32, abs_deg: f32, f: f32) -> f32 {
        let (dw, dh) = (W as usize * 2, H as usize * 4);
        let (cx, cy) = ((dw as f32 - 1.0) / 2.0, (dh as f32 - 1.0) / 2.0);
        let rad = (dw.min(dh) as f32) / 2.0 - 1.0;
        let th = abs_deg.to_radians();
        let x = cx + f * rad * th.cos();
        let y = cy + f * rad * th.sin();
        coverage(W, H, angle)[y.round() as usize][x.round() as usize].0
    }

    #[test]
    fn ten_rays_per_ring_at_thirty_six_degree_spacing() {
        // A non-zero angle, so the two rings are genuinely offset from each
        // other and a bug that ignored one ring's sign would show up.
        let rot = 12.0;
        for k in 0..10 {
            let step = 36.0 * k as f32;
            let inner = rot + 18.0 + step;
            let outer = -rot + 18.0 + step;
            assert!(probe(rot, inner, F_IN) > 0.0, "inner ray {k} missing at {inner}°");
            assert!(probe(rot, outer, F_OUT) > 0.0, "outer ray {k} missing at {outer}°");
            // Exactly halfway between two rays must be empty in both rings.
            assert_eq!(probe(rot, inner + 18.0, F_IN), 0.0, "ink between inner rays");
            assert_eq!(probe(rot, outer + 18.0, F_OUT), 0.0, "ink between outer rays");
        }
    }

    #[test]
    fn the_band_between_the_rings_is_never_lit() {
        let (dw, dh) = (W as usize * 2, H as usize * 4);
        let (cx, cy) = ((dw as f32 - 1.0) / 2.0, (dh as f32 - 1.0) / 2.0);
        let rad = (dw.min(dh) as f32) / 2.0 - 1.0;
        // Supersample offsets reach ~0.02 in f, so allow a hair of slop.
        const SLOP: f32 = 0.03;
        let mut saw_inner = false;
        let mut saw_outer = false;
        for (j, row) in coverage(W, H, 7.0).iter().enumerate() {
            for (i, (c, _)) in row.iter().enumerate() {
                if *c == 0.0 {
                    continue;
                }
                let f = ((i as f32 - cx).powi(2) + (j as f32 - cy).powi(2)).sqrt() / rad;
                assert!(f >= HUB - SLOP, "ink at f={f} inside the hub");
                assert!(f <= 1.0 + SLOP, "ink at f={f} beyond the rim");
                assert!(
                    f <= INNER_END + SLOP || f >= OUTER_START - SLOP,
                    "ink at f={f} in the gap between the rings"
                );
                if f < INNER_END {
                    saw_inner = true;
                }
                if f > OUTER_START {
                    saw_outer = true;
                }
            }
        }
        assert!(saw_inner, "inner ring drew nothing");
        assert!(saw_outer, "outer ring drew nothing");
    }

    #[test]
    fn the_rings_rotate_in_opposite_directions() {
        // At angle 0 both rings put a ray at 18 degrees.
        assert!(probe(0.0, 18.0, F_IN) > 0.0);
        assert!(probe(0.0, 18.0, F_OUT) > 0.0);
        // Advance 9 degrees: the inner ray must move to 27, the outer to 9.
        assert!(probe(9.0, 27.0, F_IN) > 0.0, "inner ring did not advance");
        assert!(probe(9.0, 9.0, F_OUT) > 0.0, "outer ring did not retreat");
        // And emphatically not the other way round. 9 and 27 are each exactly
        // halfway between the wrong ring's rays, so these must be empty.
        assert_eq!(probe(9.0, 9.0, F_IN), 0.0, "inner ring rotated backwards");
        assert_eq!(probe(9.0, 27.0, F_OUT), 0.0, "outer ring rotated forwards");
    }

    #[test]
    fn rotation_never_collapses_the_burst() {
        let base = ink(0.0);
        let mut differed = 0;
        for d in 0..360 {
            let n = ink(d as f32);
            assert!(n > 0, "burst empty at {d}°");
            assert!(n as f32 > base as f32 * 0.4, "burst collapsed to {n} dots at {d}°");
            if n != base {
                differed += 1;
            }
        }
        assert!(differed > 100, "rotation barely changes the figure ({differed} of 360)");
    }

    /// Is `fg` somewhere on the blend line from `BG_CELL` toward `tone`?
    /// Recovers `t` from the channel with the widest span and checks the other
    /// two agree, allowing for `blend`'s u8 truncation. Exact equality against
    /// sampled blend values would not work — brightness is continuous.
    pub(crate) fn is_blend_of(fg: Color, tone: Color) -> bool {
        let ch = |c: Color| match c {
            Color::Rgb(r, g, b) => [f32::from(r), f32::from(g), f32::from(b)],
            other => panic!("expected Color::Rgb, got {other:?}"),
        };
        let (bg, to, f) = (ch(theme::BG_CELL), ch(tone), ch(fg));
        let widest = (0..3)
            .max_by(|a, b| (to[*a] - bg[*a]).abs().total_cmp(&(to[*b] - bg[*b]).abs()))
            .unwrap();
        if (to[widest] - bg[widest]).abs() < 1.0 {
            return false;
        }
        let t = (f[widest] - bg[widest]) / (to[widest] - bg[widest]);
        if !(-0.02..=1.02).contains(&t) {
            return false;
        }
        (0..3).all(|k| (f[k] - (bg[k] + (to[k] - bg[k]) * t)).abs() <= 2.0)
    }

    #[test]
    fn rays_alternate_the_two_brand_tones() {
        let lines = render(W, H, 0.0);
        assert_eq!(lines.len(), H as usize);
        let (mut saw_text, mut saw_accent) = (false, false);
        for line in &lines {
            for span in &line.spans {
                if span.content == " " {
                    continue;
                }
                let fg = span.style.fg.unwrap();
                if is_blend_of(fg, theme::TEXT) {
                    saw_text = true;
                } else if is_blend_of(fg, theme::ACCENT) {
                    saw_accent = true;
                } else {
                    panic!("off-brand colour {fg:?}");
                }
            }
        }
        assert!(saw_text, "no TEXT rays");
        assert!(saw_accent, "no ACCENT rays");
    }

    #[test]
    fn empty_cells_are_spaces_and_lit_cells_are_braille() {
        let lines = render(W, H, 0.0);
        assert_eq!(lines[0].spans[0].content, " ", "empty cell must be a space, not U+2800");
        for line in &lines {
            for span in &line.spans {
                let c = span.content.chars().next().unwrap();
                assert!(
                    c == ' ' || ('\u{2801}'..='\u{28FF}').contains(&c),
                    "unexpected glyph {c:?}"
                );
            }
        }
    }
}
