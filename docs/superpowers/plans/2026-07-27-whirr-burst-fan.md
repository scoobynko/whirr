# Burst Fan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the header's rotating asterisk fan with a braille-rasterized dashed radial burst — 10 hairline rays, thermal rotation, wall-clock solid-star beat — filling the full 9-row header.

**Architecture:** A new pure module `src/ui/burst.rs` rasterizes the burst into a braille dot canvas and returns styled `Line`s; `ui/header.rs` only places it. Animation state moves from a single `fan_frame: usize` counter to two independent continuous phases on `App` (`fan_angle_deg`, `beat_phase`), both written by the main loop so rendering stays pure and every test is deterministic.

**Tech Stack:** Rust 2021, ratatui 0.29, crossterm 0.28. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-07-27-whirr-burst-fan-design.md`

## Global Constraints

- Rays: **10**, at `rot + 18° + 36°k`. Vertical rays top and bottom, no horizontal ray.
- Hub: no ink inside **26%** of radius. Dash law period `L = 0.74`, duty `DUTY = 0.86`. Ray half-thickness **0.75 dots**.
- Anti-aliasing floor: cell brightness clamped to a minimum of **0.5** so faint cells never vanish.
- Rotation: **360°/14s idle → 360°/2s hot**. Frame interval: **125 ms idle → 60 ms hot**.
- Hard invariant: **per-frame rotation must stay below 18°**, or 10-fold symmetry aliases and the star spins backwards.
- Colors: only `theme::TEXT` and `theme::ACCENT` as ray tones, blended toward `theme::BG_CELL` for AA. No other colors.
- Header height stays **9 rows** — `ui/mod.rs` is not touched. The body must not lose rows at the 30-row tier threshold.
- Empty cells render as a **space** (`' '`), never braille-blank `U+2800`, so the header isn't painted with a 21×9 block of braille.
- Repo conventions: `cargo fmt --check` fails repo-wide by design — **do not run it**. The enforced gate is `cargo clippy --all-targets -- -D warnings`. Match the surrounding compact hand-formatting style.

## Deviation from the spec (deliberate, noted)

The spec's file table puts the rasterizer in `src/ui/header.rs`. This plan puts it in a **new `src/ui/burst.rs`** instead. `header.rs` is already 260 lines; adding rasterization, AA, and color resolution would push it past 450 and mix "where things go on screen" with "how the burst is drawn". `header.rs` keeps layout; `burst.rs` owns rasterization. Everything else follows the spec.

## File Structure

| File | Responsibility |
|---|---|
| `src/ui/burst.rs` (new) | Pure burst rasterizer: dot-coverage math, braille packing, AA blending, ray tone assignment. No app state. |
| `src/ui/mod.rs` | One line: register `pub mod burst;`. |
| `src/ui/theme.rs` | Gains `blend(from, to, t)` for coverage dimming. |
| `src/ui/header.rs` | Layout only — fan column 19→21, band grows to the full 9 rows, calls `burst::render`. Compact tier derives its frame from the angle. |
| `src/app.rs` | `fan_angle_deg` + `beat_phase` replace `fan_frame`; `heat()` extracted; `tick_fan(dt)`; `fan_interval()` returns the frame interval. |
| `src/main.rs` | Feeds real elapsed `dt` to `tick_fan` and sets `beat_phase` from a wall clock. |
| `tests/render.rs` | Tier assertions switch from `✳` to a braille marker. |
| `examples/burst_preview.rs` (new, temporary) | Font-risk gate and tuning harness. Deleted in Task 7. |

---

### Task 1: Braille font gate

The spec's one un-testable risk (§9) is that braille renders badly in the real terminal. Settle it before any refactoring, so a no-go costs one throwaway file instead of the whole implementation.

**Files:**
- Create: `examples/burst_preview.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing consumed by later tasks. This is a human gate.

- [ ] **Step 1: Create the preview example**

```rust
//! Temporary font-risk gate for the braille burst fan (see
//! docs/superpowers/specs/2026-07-27-whirr-burst-fan-design.md §9).
//! Prints static braille sample rows so the real terminal font can be judged
//! before any of the fan is implemented. Deleted once the fan lands.

fn main() {
    println!("\nAll 8 dot weights (should be evenly spaced, no gaps between cells):");
    println!("⠁⠃⠇⡇⡏⡟⡿⣿⠀⠁⠃⠇⡇⡏⡟⡿⣿");

    println!("\nHairline diagonals (should read as continuous thin lines):");
    for row in [
        "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣶⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
        "⠀⠀⠀⠀⠀⠘⠦⠀⠀⠀⣭⠀⠀⠀⠴⠃⠀⠀⠀⠀⠀",
        "⠀⠀⠀⠀⠀⠀⠀⢳⡄⠀⣿⠀⢠⡞⠀⠀⠀⠀⠀⠀⠀",
        "⠀⠀⠘⠳⠀⢤⣄⣀⠹⠂⠛⠐⠏⣀⣠⡤⠀⠞⠃⠀⠀",
        "⠀⠀⠀⠀⠀⠀⠀⣉⡁⠀⠀⠀⢈⣉⠀⠀⠀⠀⠀⠀⠀",
        "⠀⠀⢠⡴⠀⠚⠋⠉⣰⠄⣤⠠⣆⠉⠙⠓⠀⢦⡄⠀⠀",
        "⠀⠀⠀⠀⠀⠀⠀⡼⠃⠀⣿⠀⠘⢧⠀⠀⠀⠀⠀⠀⠀",
        "⠀⠀⠀⠀⠀⢠⠖⠀⠀⠀⣛⠀⠀⠀⠲⡄⠀⠀⠀⠀⠀",
        "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    ] {
        println!("{row}");
    }

    println!("\nBraille beside the fallback stroke glyphs (─ ╲ │ ╱):");
    println!("⠳⣄  vs  ╲    ⣿  vs  │    ⠤⠤  vs  ──");

    println!("\nWidth check — these two rows must end at the same column:");
    println!("⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿|");
    println!("XXXXXXXXXXXXXXXXXXXXX|");
}
```

- [ ] **Step 2: Run it in the real terminal**

Run: `cargo run --example burst_preview`

Judge five things:
1. **Width** — the last two rows must end at the same column. If braille is double-width in this font, the whole approach is off and the fallback in spec §9 applies.
2. **Weight** — dots should be visible but thin. If they render as heavy blobs, hairlines will look chunky and `THICK` needs lowering in Task 3.
3. **Continuity** — the diagonals should read as lines, not scattered dots.
4. **Spacing** — no visible gutter between adjacent braille cells.
5. **Wispiness** — braille was removed from this project's charts on 2026-07-22 for reading "wispy and jagged" (`docs/superpowers/specs/2026-07-22-whirr-sparkline-charts-design.md:8`). Radial rays are a different shape from a long chart trace, and the fan gets AA dimming the charts never had, so the verdict may not carry over — but this is the check that decides it. If the burst reads wispy, stop and fall back to spec §9's stroke glyphs.

- [ ] **Step 3: Human gate — STOP here**

Report the four judgements and get an explicit go/no-go before continuing. On a no-go, abandon this plan and re-spec around the directional-stroke-glyph fallback in spec §9.

- [ ] **Step 4: Commit**

```bash
git add examples/burst_preview.rs
git commit -m "chore: braille font gate for the burst fan"
```

---

### Task 2: Color blend helper

**Files:**
- Modify: `src/ui/theme.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub fn blend(from: Color, to: Color, t: f32) -> Color` — linear RGB interpolation, `t` clamped to `0.0..=1.0`. `t = 0.0` returns `from`, `t = 1.0` returns `to`. Panics if either argument is not `Color::Rgb`. Used by `burst.rs` in Task 3.

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block at the bottom of `src/ui/theme.rs`:

```rust
    #[test]
    fn blend_hits_both_endpoints_and_the_midpoint() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(100, 200, 50);
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
        assert_eq!(blend(a, b, 0.5), Color::Rgb(50, 100, 25));
    }

    #[test]
    fn blend_clamps_out_of_range_t() {
        let a = Color::Rgb(10, 20, 30);
        let b = Color::Rgb(200, 210, 220);
        assert_eq!(blend(a, b, -1.0), a);
        assert_eq!(blend(a, b, 5.0), b);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib ui::theme`
Expected: FAIL — `cannot find function 'blend' in this scope`.

- [ ] **Step 3: Implement**

Add to `src/ui/theme.rs`, directly below the existing `gradient` function:

```rust
/// Linear RGB interpolation, used by the burst fan to dim partially covered
/// cells toward the background (its stand-in for anti-aliasing, since a cell
/// carries one foreground and no coverage information).
pub fn blend(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let ch = |c: Color| match c {
        Color::Rgb(r, g, b) => (r, g, b),
        other => panic!("blend expects Color::Rgb, got {other:?}"),
    };
    let (fr, fg, fb) = ch(from);
    let (tr, tg, tb) = ch(to);
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    Color::Rgb(lerp(fr, tr), lerp(fg, tg), lerp(fb, tb))
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --lib ui::theme`
Expected: PASS — all theme tests green.

- [ ] **Step 5: Commit**

```bash
git add src/ui/theme.rs
git commit -m "feat: rgb blend helper for burst fan anti-aliasing"
```

---

### Task 3: The burst rasterizer

The meat of the change. A pure module with no app state, so it can be tested exhaustively.

**Files:**
- Create: `src/ui/burst.rs`
- Modify: `src/ui/mod.rs` (add `pub mod burst;`)

**Interfaces:**
- Consumes: `theme::blend`, `theme::TEXT`, `theme::ACCENT`, `theme::BG_CELL` from Task 2.
- Produces, both used by Task 6 and the tests:
  - `pub fn coverage(w: u16, h: u16, angle_deg: f32, beat: f32) -> Vec<Vec<(f32, usize)>>` — per-dot `(coverage 0.0..=1.0, ray index 0..=9)`. Outer index is the dot row (`h * 4` of them), inner the dot column (`w * 2`). Ray index is meaningless where coverage is `0.0`.
  - `pub fn render(w: u16, h: u16, angle_deg: f32, beat: f32) -> Vec<Line<'static>>` — `h` lines of `w` spans each. Cells with no lit dots are `Span::raw(" ")`.

- [ ] **Step 1: Write the failing tests**

First register the module so the tests are discovered at all: add `pub mod burst;` to `src/ui/mod.rs` alongside the other `pub mod` lines.

Then create `src/ui/burst.rs` containing **only** this test module for now. Note it is `pub(crate) mod tests`, not a private one — Task 6's header tests reuse `is_blend_of` from it rather than duplicating the colour maths.

```rust
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ui::theme;

    const W: u16 = 21;
    const H: u16 = 9;

    /// Lit dots in a frame, as a count.
    fn ink(w: u16, h: u16, angle: f32, beat: f32) -> usize {
        coverage(w, h, angle, beat)
            .iter()
            .flatten()
            .filter(|(c, _)| *c > 0.0)
            .count()
    }

    /// Coverage at the dot nearest a given polar position, in the burst's own
    /// rotating frame. `f` is the fraction of the radius.
    fn probe(w: u16, h: u16, angle: f32, beat: f32, ray_deg: f32, f: f32) -> f32 {
        let (dw, dh) = (w as usize * 2, h as usize * 4);
        let (cx, cy) = ((dw - 1) as f32 / 2.0, (dh - 1) as f32 / 2.0);
        let rad = (dw.min(dh) as f32) / 2.0 - 1.0;
        let th = (angle + ray_deg).to_radians();
        let x = cx + f * rad * th.cos();
        let y = cy + f * rad * th.sin();
        coverage(w, h, angle, beat)[y.round() as usize][x.round() as usize].0
    }

    #[test]
    fn ten_rays_sit_at_thirty_six_degree_spacing() {
        // A beat phase where the travelling gap has slid off the rim, so every
        // ray is solid end to end.
        let beat = 0.98;
        for k in 0..10 {
            let a = 18.0 + 36.0 * k as f32;
            assert!(probe(W, H, 0.0, beat, a, 0.75) > 0.0, "ray {k} at {a}° missing");
            // Halfway between two rays must be empty.
            let gap = a + 18.0;
            assert_eq!(probe(W, H, 0.0, beat, gap, 0.75), 0.0, "ink at {gap}° between rays");
        }
    }

    #[test]
    fn hub_is_empty_and_rim_is_bounded() {
        let g = coverage(W, H, 0.0, 0.98);
        let (dw, dh) = (W as usize * 2, H as usize * 4);
        let (cx, cy) = ((dw - 1) as f32 / 2.0, (dh - 1) as f32 / 2.0);
        let rad = (dw.min(dh) as f32) / 2.0 - 1.0;
        for (j, row) in g.iter().enumerate() {
            for (i, (c, _)) in row.iter().enumerate() {
                if *c == 0.0 {
                    continue;
                }
                let r = ((i as f32 - cx).powi(2) + (j as f32 - cy).powi(2)).sqrt() / rad;
                assert!(r >= HUB - 0.1, "ink at r={r} inside the hub");
                assert!(r <= 1.0 + 0.1, "ink at r={r} beyond the rim");
            }
        }
    }

    #[test]
    fn rotation_moves_the_figure_without_dropping_out() {
        let base = ink(W, H, 0.0, 0.5);
        let mut differed = 0;
        // A full turn in 1-degree steps: the burst must never collapse and
        // must actually change. 18 deg is one ray spacing, so 360 steps cover
        // every phase relative to the dot grid.
        for d in 0..360 {
            let n = ink(W, H, d as f32, 0.5);
            assert!(n > 0, "burst empty at {d}°");
            assert!(n as f32 > base as f32 * 0.4, "burst collapsed to {n} dots at {d}°");
            if n != base {
                differed += 1;
            }
        }
        assert!(differed > 100, "rotation barely changes the figure ({differed} of 360)");
    }

    #[test]
    fn beat_produces_both_a_solid_and_a_broken_frame() {
        let inks: Vec<usize> = (0..40).map(|i| ink(W, H, 0.0, i as f32 / 40.0)).collect();
        let max = *inks.iter().max().unwrap();
        let min = *inks.iter().min().unwrap();
        assert!(min > 0, "beat empties the burst");
        // The travelling gap has to make a visible difference, but the
        // reference keeps total ink near-constant, so this is a modest range.
        assert!(max as f32 > min as f32 * 1.1, "beat is invisible: {min}..{max}");
        assert!(max as f32 < min as f32 * 2.5, "beat blinks: {min}..{max}");
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
            .max_by(|a, b| {
                (to[*a] - bg[*a]).abs().total_cmp(&(to[*b] - bg[*b]).abs())
            })
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
        let lines = render(W, H, 0.0, 0.98);
        assert_eq!(lines.len(), H as usize);
        let mut saw_text = false;
        let mut saw_accent = false;
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
    fn empty_cells_are_spaces_not_braille_blank() {
        let lines = render(W, H, 0.0, 0.5);
        let corner = &lines[0].spans[0];
        assert_eq!(corner.content, " ", "empty cell must be a space, not U+2800");
    }

    #[test]
    fn every_lit_cell_is_a_braille_glyph() {
        for line in render(W, H, 0.0, 0.5) {
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib ui::burst`
Expected: FAIL to compile — `cannot find function 'coverage'`, `cannot find function 'render'`, `cannot find value 'HUB'`.

- [ ] **Step 3: Implement the rasterizer**

Add above the test module in `src/ui/burst.rs`:

```rust
//! Dashed radial burst for the header, rasterized onto a braille dot canvas.
//!
//! Ported from a reference recording measured frame by frame: 10 hairline rays
//! at 36° spacing (vertical rays top and bottom, no horizontal ray), an empty
//! hub, and a single gap per ray that travels outward until it slides off the
//! rim and the whole star reads solid. See
//! `docs/superpowers/specs/2026-07-27-whirr-burst-fan-design.md`.
//!
//! Braille dots are roughly square, so unlike the old cell-grid fan this needs
//! no aspect correction — the burst rasterizes as a true circle.

use ratatui::prelude::*;

use super::theme;

/// Rays, and the angular spacing that follows from it.
pub const RAYS: usize = 10;
const RAY_STEP: f32 = 360.0 / RAYS as f32;
/// First ray offset: puts rays at ±18°, ±54°, ±90°, ±126°, ±162° — vertical
/// rays top and bottom, no horizontal ray, exactly as measured.
const RAY_OFFSET: f32 = 18.0;
/// No ink inside this fraction of the radius.
pub const HUB: f32 = 0.26;
/// Dash pattern period and duty along the normalised radius. One gap per ray.
const DASH_L: f32 = 0.74;
const DASH_DUTY: f32 = 0.86;
/// Ray half-thickness in dots — hairlines, one dot wide at every radius.
const THICK: f32 = 0.75;
/// Supersampling per dot, per axis.
const SS: i32 = 3;
/// A dot lights at this coverage.
const DOT_ON: f32 = 0.4;
/// Cells never dim below this, so faint ones survive a light terminal.
const MIN_BRIGHT: f32 = 0.5;

/// Per-dot `(coverage, ray index)` for one frame. `angle_deg` rotates the whole
/// figure; `beat` is the dash phase in `0.0..1.0`.
pub fn coverage(w: u16, h: u16, angle_deg: f32, beat: f32) -> Vec<Vec<(f32, usize)>> {
    let (dw, dh) = (w as usize * 2, h as usize * 4);
    let (cx, cy) = ((dw as f32 - 1.0) / 2.0, (dh as f32 - 1.0) / 2.0);
    let rad = (dw.min(dh) as f32) / 2.0 - 1.0;
    let phase = beat.rem_euclid(1.0) * DASH_L;
    let step = 1.0 / SS as f32;
    (0..dh)
        .map(|j| {
            (0..dw)
                .map(|i| {
                    let mut hits = 0;
                    let mut ray = 0;
                    for a in 0..SS {
                        for b in 0..SS {
                            let x = i as f32 + (a as f32 + 0.5) * step - 0.5 - cx;
                            let y = j as f32 + (b as f32 + 0.5) * step - 0.5 - cy;
                            if let Some(k) = sample(x, y, rad, angle_deg, phase) {
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
/// the centre; the ray index is taken in the rotating frame so a ray keeps its
/// index — and therefore its tone — as the figure turns.
fn sample(x: f32, y: f32, rad: f32, angle_deg: f32, phase: f32) -> Option<usize> {
    let r = x.hypot(y);
    let f = r / rad;
    if f < HUB || f > 1.0 {
        return None;
    }
    let a = y.atan2(x).to_degrees() - angle_deg;
    let k = ((a - RAY_OFFSET) / RAY_STEP).round();
    let off = a - (RAY_OFFSET + RAY_STEP * k);
    // Perpendicular distance from the ray's centre line, in dots.
    if (off.to_radians().sin().abs() * r) > THICK {
        return None;
    }
    if ((f - HUB - phase) / DASH_L).rem_euclid(1.0) >= DASH_DUTY {
        return None;
    }
    Some((k as i32).rem_euclid(RAYS as i32) as usize)
}

/// Braille dot bit for each `(row, col)` within a cell.
const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// One frame, as `h` lines of `w` spans. Empty cells are plain spaces.
pub fn render(w: u16, h: u16, angle_deg: f32, beat: f32) -> Vec<Line<'static>> {
    let g = coverage(w, h, angle_deg, beat);
    (0..h as usize)
        .map(|cy| {
            let spans: Vec<Span> = (0..w as usize)
                .map(|cx| {
                    let mut bits = 0u8;
                    let mut sum = 0.0;
                    let mut lit = 0;
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
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --lib ui::burst`
Expected: PASS — all 7 burst tests green.

If `ten_rays_sit_at_thirty_six_degree_spacing` fails on the between-rays assertion, `THICK` is too large for this radius; the fix is lowering `THICK`, not widening the test's tolerance. If `beat_produces_both_a_solid_and_a_broken_frame` fails the "invisible" assertion, lower `DASH_DUTY` toward 0.80.

- [ ] **Step 5: Check the per-frame cost against budget**

The spec's power claim assumes the burst costs well under the 151 µs full-UI redraw. `SS = 3` means 9 trig evaluations per dot, 1512 dots at 21×9 — worth measuring rather than assuming.

Create `examples/burst_bench.rs`:

```rust
use std::time::Instant;

fn main() {
    for _ in 0..200 {
        std::hint::black_box(whirr::ui::burst::render(21, 9, 0.0, 0.5));
    }
    let n = 2000;
    let s = Instant::now();
    for i in 0..n {
        std::hint::black_box(whirr::ui::burst::render(21, 9, i as f32, 0.5));
    }
    println!("burst render: {:.1} us/frame", s.elapsed().as_secs_f64() * 1e6 / f64::from(n));
}
```

Run: `cargo run --release --example burst_bench`
Expected: a number under **200 µs/frame**.

**If it exceeds 200 µs**, replace the supersampling in `coverage` with distance-based coverage — one evaluation per dot instead of nine. Change `SS` to `1` and, in `sample`, return a coverage value instead of a boolean by replacing the thickness test with:

```rust
    let dperp = off.to_radians().sin().abs() * r;
    let cov = (THICK + 0.5 - dperp).clamp(0.0, 1.0);
    if cov <= 0.0 {
        return None;
    }
```

then have `sample` return `Option<(f32, usize)>` and `coverage` use that value directly. Re-run the tests; they are written against the public interface and should still pass.

- [ ] **Step 6: Remove the bench and commit**

```bash
rm examples/burst_bench.rs
git add src/ui/burst.rs src/ui/mod.rs
git commit -m "feat: braille burst rasterizer with 10 hairline rays"
```

---

### Task 4: Continuous animation state

**Files:**
- Modify: `src/app.rs:40` (field), `src/app.rs:62` (initialiser), `src/app.rs:224-243` (`fan_interval`/`tick_fan`), `src/app.rs:345-352` (the old wrap test)

**Interfaces:**
- Consumes: nothing.
- Produces, all used by Tasks 5 and 6:
  - `pub fan_angle_deg: f32` — burst rotation, wrapped to `0.0..360.0`. Replaces `fan_frame`.
  - `pub beat_phase: f32` — dash phase in `0.0..1.0`, written by the main loop from a wall clock.
  - `pub fn heat(&self) -> f32` — `0.0..=1.0`, temperature 55→95 °C with CPU-load fallback.
  - `pub fn fan_interval(&self) -> Duration` — frame interval, 125 ms at `heat == 0.0` down to 60 ms at `heat == 1.0`.
  - `pub fn tick_fan(&mut self, dt: Duration)` — advances `fan_angle_deg` by ω(heat)·dt and sets `dirty`. Signature change: it took no argument before.

- [ ] **Step 1: Write the failing tests**

Replace the existing `fan_frame_wraps_at_twenty_four` test in `src/app.rs`'s `mod tests` with:

```rust
    #[test]
    fn heat_tracks_temperature_and_falls_back_to_load() {
        let mut a = App::new(false);
        assert_eq!(a.heat(), 0.0, "no samples yet");
        a.ingest(Snapshot::Medium(demo_medium(40.0)));
        assert_eq!(a.heat(), 0.0, "40C is below the 55C floor");
        a.ingest(Snapshot::Medium(demo_medium(95.0)));
        assert_eq!(a.heat(), 1.0, "95C is the ceiling");
        a.ingest(Snapshot::Medium(demo_medium(75.0)));
        assert!((a.heat() - 0.5).abs() < 0.01, "75C is halfway");
    }

    #[test]
    fn fan_interval_ramps_from_125ms_to_60ms() {
        let mut a = App::new(false);
        assert_eq!(a.fan_interval(), Duration::from_millis(125));
        a.ingest(Snapshot::Medium(demo_medium(95.0)));
        assert_eq!(a.fan_interval(), Duration::from_millis(60));
    }

    #[test]
    fn tick_fan_never_turns_more_than_eighteen_degrees_per_frame() {
        // 10-fold symmetry: above 18 deg/frame the burst aliases and appears
        // to spin backwards. This must hold across the whole thermal range.
        for temp in [40.0, 55.0, 65.0, 75.0, 85.0, 95.0, 110.0] {
            let mut a = App::new(false);
            a.ingest(Snapshot::Medium(demo_medium(temp)));
            let dt = a.fan_interval();
            a.fan_angle_deg = 0.0;
            a.tick_fan(dt);
            assert!(
                a.fan_angle_deg < 18.0,
                "{temp}C turns {}deg per frame — aliases",
                a.fan_angle_deg
            );
        }
    }

    #[test]
    fn tick_fan_spins_faster_when_hot_and_wraps_at_360() {
        let mut cold = App::new(false);
        cold.ingest(Snapshot::Medium(demo_medium(40.0)));
        let mut hot = App::new(false);
        hot.ingest(Snapshot::Medium(demo_medium(95.0)));
        let dt = Duration::from_millis(100);
        cold.tick_fan(dt);
        hot.tick_fan(dt);
        assert!(hot.fan_angle_deg > cold.fan_angle_deg * 3.0, "hot fan should be much faster");

        let mut a = App::new(false);
        a.fan_angle_deg = 359.0;
        a.ingest(Snapshot::Medium(demo_medium(95.0)));
        a.tick_fan(Duration::from_millis(100));
        assert!(a.fan_angle_deg < 360.0, "angle must wrap, got {}", a.fan_angle_deg);
    }

    #[test]
    fn cold_fan_takes_about_fourteen_seconds_per_revolution() {
        let mut a = App::new(false);
        a.ingest(Snapshot::Medium(demo_medium(40.0)));
        a.tick_fan(Duration::from_secs(1));
        // 360/14 = 25.7 deg/s
        assert!((a.fan_angle_deg - 25.7).abs() < 0.5, "got {} deg/s", a.fan_angle_deg);
    }
```

Add this helper inside the same `mod tests` block, next to the existing `demo_fast`:

```rust
    fn demo_medium(temp_c: f32) -> MediumSnap {
        MediumSnap {
            temp_c: Some(temp_c),
            power: None,
            battery: None,
            memory: None,
            uptime_secs: 3600,
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib app`
Expected: FAIL to compile — `no method named 'heat'`, `no field 'fan_angle_deg'`, and `this method takes 1 argument but 0 were supplied` for `tick_fan`.

- [ ] **Step 3: Implement**

In `src/app.rs`, replace the `pub fan_frame: usize,` field (line 40) with:

```rust
    /// Burst rotation in degrees, wrapped to `0.0..360.0`. Thermal: the hotter
    /// the machine, the faster it turns.
    pub fan_angle_deg: f32,
    /// Burst dash phase in `0.0..1.0`. Wall-clock, deliberately *not* thermal —
    /// the reference's ~2s solid-star beat is load-independent. Written by the
    /// main loop so rendering stays pure and tests stay deterministic.
    pub beat_phase: f32,
```

Replace `fan_frame: 0,` in `new` (line 62) with:

```rust
            fan_angle_deg: 0.0,
            beat_phase: 0.0,
```

Replace the whole of `fan_interval` and `tick_fan` (lines 224-243) with:

```rust
    /// Simulated Mac fan curve: lazy below ~55°C, ramping steeply toward
    /// 95°C — temperature is what actually drives real fans. Falls back to CPU
    /// load when the machine has no usable temp sensor.
    pub fn heat(&self) -> f32 {
        match self.medium.as_ref().and_then(|m| m.temp_c) {
            Some(t) => ((t - 55.0) / 40.0).clamp(0.0, 1.0),
            None => self.fast.as_ref().map_or(0.0, |f| (f.total_cpu / 100.0).clamp(0.0, 1.0)),
        }
    }

    /// Redraw interval for the burst: 125ms idle down to 60ms hot. The frame
    /// rate has to rise with the spin, not just the spin itself — the burst is
    /// 10-fold symmetric, so anything past 18°/frame aliases into a backwards
    /// spin. At 60ms/125ms this stays at 10.8°/3.2° per frame.
    pub fn fan_interval(&self) -> Duration {
        Duration::from_millis((125.0 - 65.0 * f64::from(self.heat())) as u64)
    }

    /// Advance the burst rotation over `dt` of real time: 360°/14s idle up to
    /// 360°/2s hot, matching the perceived range of the old stepped fan.
    pub fn tick_fan(&mut self, dt: Duration) {
        const COLD_DPS: f32 = 360.0 / 14.0;
        const HOT_DPS: f32 = 360.0 / 2.0;
        let dps = COLD_DPS + (HOT_DPS - COLD_DPS) * self.heat();
        self.fan_angle_deg = (self.fan_angle_deg + dps * dt.as_secs_f32()).rem_euclid(360.0);
        self.dirty = true;
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --lib app`
Expected: PASS. `src/main.rs` and `src/ui/header.rs` will not compile yet — that is expected and fixed in Tasks 5 and 6. Use `cargo test --lib app` (not a bare `cargo test`) until then.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: continuous fan angle and wall-clock beat phase"
```

---

### Task 5: Main-loop wiring

**Files:**
- Modify: `src/main.rs:71-97`

**Interfaces:**
- Consumes: `App::tick_fan(dt)`, `App::fan_interval()`, `App::beat_phase` from Task 4.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Implement**

This task has no unit test — it is loop plumbing over a real clock, and everything it computes is already covered in Task 4. The verification is the integration test suite in Task 6 plus a manual run.

In `src/main.rs`, replace line 71:

```rust
    let mut last_fan = std::time::Instant::now();
```

with:

```rust
    let mut last_fan = std::time::Instant::now();
    // The dash beat runs on a wall clock, independent of the thermal spin —
    // it stays a steady ~2s breath however hard the machine is working.
    let started = std::time::Instant::now();
    const BEAT_SECS: f32 = 2.0;
```

Then replace the tick block at lines 94-97:

```rust
        if !app.no_fan && last_fan.elapsed() >= app.fan_interval() {
            app.tick_fan();
            last_fan = std::time::Instant::now();
        }
```

with:

```rust
        if !app.no_fan && last_fan.elapsed() >= app.fan_interval() {
            let now = std::time::Instant::now();
            app.tick_fan(now - last_fan);
            app.beat_phase = (started.elapsed().as_secs_f32() / BEAT_SECS).fract();
            last_fan = now;
        }
```

Passing the measured `now - last_fan` rather than `fan_interval()` keeps the spin rate honest when the loop runs late — a blocked redraw or a slow terminal shows up as a longer `dt`, not as a slower fan.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: `src/main.rs` errors are gone. `src/ui/header.rs` still fails on `app.fan_frame` — expected, fixed in Task 6.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: drive the fan from real elapsed time and a wall clock beat"
```

---

### Task 6: Header integration

**Files:**
- Modify: `src/ui/header.rs:24-40` (constants), `src/ui/header.rs:52-111` (both tiers), `src/ui/header.rs:113-151` (delete `render_star_fan`), `src/ui/header.rs:171-258` (tests)
- Modify: `tests/render.rs:93-115`

**Interfaces:**
- Consumes: `burst::render` from Task 3; `App::fan_angle_deg`, `App::beat_phase` from Task 4.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the failing tests**

In `src/ui/header.rs`, delete the three tests `star_fan_rotates_between_adjacent_frames`, `full_tier_needs_nine_rows_for_unclipped_star_fan`, and `star_fan_alternates_the_two_brand_tones`, and add:

```rust
    /// Any braille glyph with at least one dot. `U+2800` is the blank and must
    /// never be emitted — empty cells are plain spaces.
    fn has_braille(s: &str) -> bool {
        s.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
    }

    #[test]
    fn full_tier_needs_nine_rows_for_the_burst() {
        assert!(has_braille(&draw_header(80, 9)), "burst missing at height 9");
        for h in [5, 6, 7, 8] {
            assert!(!has_braille(&draw_header(80, h)), "height {h} must fall back to compact");
        }
    }

    #[test]
    fn burst_fills_the_whole_nine_row_band() {
        let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let mut app = App::demo();
        app.beat_phase = 0.98;
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        // The vertical rays reach the top and bottom rows of the header.
        for y in [0u16, 8] {
            let row: String = (0..80).map(|x| buf[(x, y)].symbol()).collect();
            assert!(has_braille(&row), "no burst ink in header row {y}");
        }
    }

    #[test]
    fn burst_rotates_between_angles() {
        let draw = |deg: f32| -> String {
            let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
            let mut app = App::demo();
            app.fan_angle_deg = deg;
            app.beat_phase = 0.5;
            t.draw(|fr| super::render(fr, fr.area(), &app)).unwrap();
            t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
        };
        let base = draw(0.0);
        for deg in [9.0, 18.0, 27.0] {
            assert_ne!(base, draw(deg), "{deg}° renders identically to 0°");
        }
    }

    #[test]
    fn burst_uses_only_blends_of_the_two_brand_tones() {
        let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut saw_text = false;
        let mut saw_accent = false;
        for cell in buf.content() {
            let c = cell.symbol().chars().next().unwrap();
            if !('\u{2801}'..='\u{28FF}').contains(&c) {
                continue;
            }
            let fg = cell.style().fg.unwrap();
            // Reuse the blend-line check from the burst module's tests.
            if crate::ui::burst::tests::is_blend_of(fg, theme::TEXT) {
                saw_text = true;
            } else if crate::ui::burst::tests::is_blend_of(fg, theme::ACCENT) {
                saw_accent = true;
            } else {
                panic!("off-brand colour {fg:?}");
            }
        }
        assert!(saw_text && saw_accent, "both brand tones must appear");
    }

    #[test]
    fn no_fan_leaves_the_header_free_of_braille() {
        let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let mut app = App::demo();
        app.no_fan = true;
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        assert!(!has_braille(&s), "--no-fan must not draw the burst");
    }
```

Add `use crate::ui::theme;` to the test module's imports — `super::theme` will not resolve, since header's own `use super::theme;` is a private import and is not re-exported to the nested test module. The existing tests in this file use the full `crate::ui::theme::ACCENT` path for the same reason.

In `tests/render.rs`, replace the three `✳` assertions. Add this helper near `draw_at`:

```rust
fn has_braille(s: &str) -> bool {
    s.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
}
```

Then change line 96 to `assert!(has_braille(&c), "burst fan missing");`, line 105 to `assert!(!has_braille(&c), "burst fan must not render at 80x24");`, and lines 112-114 to:

```rust
    assert!(has_braille(&draw_at(120, 30)), "120x30 must be full tier");
    assert!(!has_braille(&draw_at(119, 30)), "119x30 must be compact");
    assert!(!has_braille(&draw_at(120, 29)), "120x29 must be compact");
```

Rename the test at line 94 from `full_tier_shows_hero_font_and_star_fan` to `full_tier_shows_hero_font_and_burst_fan`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test`
Expected: FAIL to compile — `no field 'fan_frame' on type 'App'` in `header.rs`.

- [ ] **Step 3: Implement**

In `src/ui/header.rs`, delete the `FAN_H`, `FAN_W`, and `FAN_ROT_FRAMES` constants (lines 38-40) and the whole `render_star_fan` function (lines 113-151). Update the comment block above them (lines 32-37) to:

```rust
// Full-tier fan: a dashed radial burst on a braille dot canvas — see
// `ui/burst.rs`. It fills the header's full 9 rows; the logo and facts keep
// the rows they always had, so only the fan grew.
const FAN_COLS: u16 = 21;
```

Add `use super::burst;` to the imports at the top.

Replace `render_full` (lines 52-83) with:

```rust
fn render_full(f: &mut Frame, area: Rect, app: &App) {
    // The burst claims the whole 9-row header. The logo and facts stay on the
    // rows they occupied when the band was 7 rows with a pad above, so this
    // change is invisible to everything except the fan.
    let cols = Layout::horizontal([
        Constraint::Length(26),        // logo
        Constraint::Length(FAN_COLS),  // burst fan
        Constraint::Min(0),            // ambient facts
    ])
    .split(area);

    let logo_area = Rect { y: area.y + 2, height: area.height.saturating_sub(2).min(4), ..cols[0] };
    let logo_lines: Vec<Line> = LOGO4
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(theme::ACCENT).bold()))
        .collect();
    f.render_widget(Paragraph::new(logo_lines), logo_area);

    if !app.no_fan {
        let lines = burst::render(cols[1].width, cols[1].height, app.fan_angle_deg, app.beat_phase);
        f.render_widget(Paragraph::new(lines), cols[1]);
    }

    let facts_area = Rect {
        y: area.y + 3,
        height: area.height.saturating_sub(3).min(3),
        ..cols[2]
    };
    f.render_widget(facts_paragraph(app), facts_area);
}
```

In `render_compact` (line 102), replace the frame lookup:

```rust
        let frame = FAN_FRAMES[(app.fan_frame / 2) % 4];
```

with:

```rust
        // The compact fan keeps its 4 hand-drawn frames; a quarter turn of the
        // burst's angle advances it by one.
        let frame = FAN_FRAMES[(app.fan_angle_deg / 90.0) as usize % 4];
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test`
Expected: PASS — the whole suite, unit and integration.

- [ ] **Step 5: Run the clippy gate**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no warnings. Do **not** run `cargo fmt` — it fails repo-wide by design.

- [ ] **Step 6: Commit**

```bash
git add src/ui/header.rs tests/render.rs
git commit -m "feat: burst fan fills the full header band"
```

---

### Task 7: Visual pass and cleanup

**Files:**
- Delete: `examples/burst_preview.rs`
- Modify: `src/ui/burst.rs` (tuning constants only, if needed)

**Interfaces:**
- Consumes: everything above.
- Produces: nothing.

- [ ] **Step 1: Run the real binary and watch it**

Run: `cargo run --release`

Check against the reference recording (`~/Downloads/whirr-anim.mp4`):
1. **10 rays**, clearly separated, vertical rays top and bottom.
2. **Rotation direction is consistent** — no backwards or stuttering spin. This is the alias check; if it fails, the per-frame angle is over 18° and Task 4's guard has a hole.
3. **The gap travels outward** and the star momentarily goes solid roughly every 2 seconds.
4. **Two tones alternate** and travel with their rays.
5. The burst does not look cramped against the header's top and bottom edges.

- [ ] **Step 2: Load the machine and re-watch**

Run: `yes > /dev/null & yes > /dev/null & sleep 60; kill %1 %2`

While that runs, confirm the burst speeds up smoothly and the ~2s beat stays at its own steady pace, unaffected by the spin. Then confirm it slows back down after the load stops.

- [ ] **Step 3: Tune if needed**

Only these constants in `src/ui/burst.rs` are tuning knobs — geometry is fixed by the spec:

| Symptom | Knob |
|---|---|
| Rays look chunky or blur together near the hub | lower `THICK` from `0.75` |
| Rays look broken or too faint | raise `THICK`, or lower `DOT_ON` from `0.4` |
| The beat is hard to see | lower `DASH_DUTY` from `0.86` |
| The beat reads as a blink rather than a travelling gap | raise `DASH_DUTY`, or raise `DASH_L` from `0.74` |
| Dim cells disappear on your terminal background | raise `MIN_BRIGHT` from `0.5` |

After any change: `cargo test --lib ui::burst`.

- [ ] **Step 4: Delete the preview example**

```bash
rm examples/burst_preview.rs
rmdir examples 2>/dev/null || true
```

- [ ] **Step 5: Full verification**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass, no clippy warnings.

Run: `cargo run --release -- --no-fan`
Expected: header renders with logo and facts, no burst, and the UI only redraws on new data or a keypress.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: tune the burst fan and drop the preview harness"
```

---

## Verification checklist

- [ ] `cargo test` — all unit and integration tests pass
- [ ] `cargo clippy --all-targets -- -D warnings` — clean
- [ ] Burst renders at 120×30 and above; never at 119×30, 120×29, or 80×24
- [ ] `--no-fan` draws no braille
- [ ] Rotation never appears to reverse across the full thermal range
- [ ] The ~2s beat holds its pace while the spin speeds up under load
- [ ] Header is still 9 rows; the body did not lose rows at 30-row height
- [ ] `examples/` is gone
