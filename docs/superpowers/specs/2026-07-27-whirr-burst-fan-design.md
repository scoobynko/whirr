# whirr burst fan — design

Date: 2026-07-27
Status: approved

## Goal

Replace the header's rotating asterisk fan with a **radial burst of
counter-rotating ray halves**, rendered with braille sub-cell hairlines and
coverage-based anti-aliasing.

The burst's geometry is ported from a reference animation
(`~/Downloads/whirr-anim.mp4`); its *motion* deliberately is not. See
"Motion: departure from the reference" below.

## Reference analysis

The reference was measured frame-by-frame (456 frames, 30 fps, 15.2 s) rather
than eyeballed. Two independent methods (angular cross-correlation and
single-ray centroid tracking) agree on the rotation rate.

| Property | Measured value |
|---|---|
| Rays | **10**, spaced 36° (measured 35.5–36.2°) |
| Ray angles | 18° + 36°k — vertical rays top and bottom, **no** horizontal ray |
| Rotation | **8.3°/s ≈ 43 s per revolution**, clockwise, perfectly steady |
| Ray structure | dashed; 2–3 dashes per ray, hub empty to ~26% of radius |
| Dash motion | dashes slide outward; gaps open and close |
| Beat | every **~2.1 s** the dashes close up and the ray flashes solid, then breaks apart |
| Ink | total lit length constant to within 1.2% — nothing fades, it only slides |
| Color | flat white 1px stroke on near-black; no color, no trails, no opacity ramp |

The rotation is slow enough (17° in a 2-second window, under half a ray
spacing) that it reads as static in any short sample. It is real.

## Decisions (validated with user)

1. **Speed stays thermal.** Port the reference's look, but keep spin rate
   driven by temperature as today. The reference's own 43 s/rev is slower than
   whirr's coldest idle state and would drop the fan's readout value.
2. **Counter-rotating halves, no pulse.** Each ray splits into an inner and an
   outer segment that rotate in *opposite* directions at equal speed. The
   reference's travelling dash gap and wall-clock solid-star beat are both
   dropped. Validated in motion against a slower-outer-ring variant (−0.4×);
   equal-and-opposite won.
3. **Braille rasterization + anti-aliasing.** Sub-cell hairlines at true ray
   angles, not directional stroke glyphs. Accepted cost: appearance varies with
   terminal font.
4. **19 × 7 cells, centred in the 9-row header band.** Height is the only lever
   on burst size (see §2). A full-band 21×9 version was built and judged too
   large; this leaves a blank row above and below.
5. **Two-tone alternating rays.** 10 rays alternate TEXT and ACCENT, five each,
   tone travelling with the ray — the existing e2b-style split, which 10
   divides evenly.

## 1. What the terminal cannot reproduce

Recorded so future work doesn't relitigate it:

- **Resolution.** The reference is a 1px stroke on a 1080px canvas — 0.09% of
  the diameter. The finest terminal mark is one braille dot ≈ 3.6% of the
  diameter, ~40× coarser. This is why a ray holds one travelling gap instead of
  the reference's finer dash pattern.
- **One color per cell.** Braille gives 8 sub-dots but a single foreground.
  Where two rays share a cell, one tone wins.
- **Binary coverage.** No true anti-aliasing; §4 approximates it per-cell.
- **Font dependence.** Braille dot weight and cell advance vary by terminal
  font. This is a portability risk, not a capability ceiling.

Not limits, but choices made here: frame rate (30 fps would cost ~0.5% of a
core), rotation speed, footprint, and monochrome-vs-two-tone.

## 2. Geometry — `src/ui/header.rs`

The burst is a circle and the header band is short, so **height is the only
lever on its size**. Widening the column does nothing: at 27 columns × 8 rows
the burst is 30 dots wide inside a 54-dot box.

- Fan is **19 × 7 cells, centred in the 9-row header** — one blank row above
  and below. Header height stays 9 — no change to `ui/mod.rs`, no rows taken
  from the body at the 30-row tier threshold.
- Canvas is 38 × 28 dots; burst radius **13 dots**.
- Sized by eye against the wordmark on 2026-07-27, animated, alongside a 21×9
  (radius 17, filling the whole band) and a 15×5 (radius 9) candidate. 21×9 was
  built first and judged too large; 15×5 is past the floor — below roughly
  radius 10 the two rings merge and the counter-rotation stops reading.
- Braille dots are approximately square, so the burst rasterizes as a true
  circle with no aspect correction — unlike the current cell-grid fan, which
  needs a 2.2× column stretch.
- **10 rays** per ring at `rot + 18° + 36°k`.
- **Hub**: no ink inside 26% of radius (~4.4 dots).
- **Hairline thickness**: half-thickness 0.75 dots, measured as perpendicular
  distance to the ray, so rays stay one dot wide at every radius.

Logo and facts stay on the rows they have always occupied (`area.y + 2` and
`area.y + 3`); only the fan's footprint changed.

## 3. Two counter-rotating rings

Each ray is split into two segments by normalised radius `f`:

```
inner ring:  HUB (0.26) <= f <= INNER_END (0.58)      turns +angle
outer ring:  OUTER_START (0.68) <= f <= 1.0           turns -angle
gap:         0.58 < f < 0.68                          never lit
```

The gap is what makes the split read as two halves of one blade rather than a
smear. Both rings carry the full 10 rays; only the angle applied differs, so a
point's ring decides which rotation it follows.

Because the rings shear against each other at twice the spin rate, they line up
every `36° / 2ω` and the blade briefly reads continuous from hub to rim. That
recovers the reference's solid-star moment **from geometry**, with no second
clock and no separate animation state — which is why the dash law and the
wall-clock beat are both gone.

### Motion: departure from the reference

The reference animates a dash gap travelling outward along each ray, with the
whole figure rotating slowly as one rigid body. This design keeps the
reference's *geometry* (10 rays, 36° spacing, empty hub, hairline strokes) and
replaces its *motion*. Validated in motion at 21×9 braille against a
slower-outer-ring variant; equal-and-opposite was chosen.

`INNER_END` and `OUTER_START` are tuning constants — the two-ring split is what
is fixed.

## 4. Anti-aliasing

Terminal glyphs are binary, so coverage is approximated per cell:

1. Each braille dot is **3×3 supersampled**; the dot lights at ≥40% coverage.
2. Each cell's brightness is the **mean coverage of its lit dots**.
3. The cell's foreground blends from `theme::BG_CELL` toward the ray tone by
   that brightness, **floored at 0.5** so faint cells never vanish against a
   terminal background that may not be dark.

`ui/theme.rs` gains a blend helper for this.

## 5. Color

10 rays alternate `theme::TEXT` (5) and `theme::ACCENT` (5); the tone travels
with the ray as it rotates. A cell containing dots from two rays takes the tone
of whichever ray owns more of its lit dots. This only occurs in a thin band
just outside the hub, where the 36° spacing is ~2.8 dots of arc.

## 6. Motion and timing

**One clock, one accumulator.** `fan_angle_deg` is the inner ring's angle; the
outer ring is simply its negation. There is no second animation phase and no
wall-clock timer.

Angular velocity ramps `360°/14s` idle → `360°/2s` hot. The heat term formerly
inlined in `fan_interval()` (temperature 55→95 °C, falling back to CPU load
when no sensor reads) is factored out into its own `heat()` helper, since both
the frame interval and ω now consume it. Rotation integrates against real
elapsed time rather than counting fixed steps, so a loop running late shows up
as a longer `dt`, not a slower fan.

**Frame interval scales 125 ms idle → 60 ms hot.** This is forced, not
cosmetic: each ring is 10-fold symmetric, so any per-ring rotation above 18°
per frame aliases and that ring appears to spin backwards. At a fixed 8 fps the
hot end (2 s/rev) would need 45°/frame. The scaled interval gives 3.2°/frame
idle and 10.8°/frame hot, both safely under the limit.

**Known and accepted:** the *alignment shimmer* (§3) is a relative effect at
twice the per-ring rate — 22.5°/frame at the hot end, past the 18° limit. So
the moments where inner and outer line up blur out under heavy load. Each ring
still rotates coherently and in the correct direction; only the shimmer is lost,
and at 2 s/rev everything reads as speed anyway.

Cost: worst case 16.7 fps × 151 µs per full 160×44 redraw ≈ **0.25% of one
core** (measured). The terminal receives only the changed fan cells.

## 7. Code changes

| File | Change |
|---|---|
| `src/app.rs` | Replace `fan_frame: usize` with `fan_angle_deg: f32`. Extract `heat()`. `tick_fan(dt: Duration)` integrates ω(heat) and wraps the angle. `fan_interval()` returns the frame interval (125→60 ms). |
| `src/main.rs` | Pass real elapsed `dt` to `tick_fan`. |
| `src/ui/burst.rs` | New: the two-ring braille rasterizer. |
| `src/ui/header.rs` | Call `burst::render` in place of `render_star_fan`. Fan is 19×7 cells, centred in the existing 9-row header band with a blank row above and below. |
| `src/ui/theme.rs` | Blend helper for coverage dimming. |
| `tests/render.rs` | Replace the three `✳` tier assertions with a braille marker check. |

The angle lives as a plain `App` field rather than being read from a clock
inside render, so rendering stays pure and every test is deterministic.

Unchanged: `--no-fan`, and the compact-tier 4-frame ASCII fan below the full
tier — its frame derives from the angle as `(angle / 90) % 4`.

## 8. Tests

- **Ray geometry**: 10 distinct ray directions at 36° spacing, in both rings.
- **Ring split**: the gap band (0.58 < f < 0.68) is never lit, and both rings
  carry ink.
- **Counter-rotation**: as the angle advances, inner-ring ink moves one way in
  angle and outer-ring ink the other.
- **Rotation robustness**: renders differ between angles; sweeping 0→360° never
  drops below a minimum ink floor (catches strobing and dropout).
- **Two-tone**: only TEXT and ACCENT appear as base tones, and both are present.
- **Anti-aliasing**: every lit cell's color lies on the blend line between
  `BG_CELL` and its ray tone.
- **Tier**: braille renders at ≥120×30 and never at 80×24 or 119×30.

## 9. Risks

**Braille rendering across terminal fonts — gate passed, residual risk
accepted.** Checked live on 2026-07-27 in the author's terminal against
directional stroke glyphs (`─ ╲ │ ╱`) at 21×9, both animated: braille was
judged clearly better. Braille dots are lighter and more separated in this font
than the reference's hairlines, so the burst reads as a fine stipple rather
than solid strokes — that look was chosen deliberately, not settled for.

The residual risk is that other users' fonts render braille worse: heavier
dots, wider gutters, or misaligned cells. whirr is planned for crates.io and
Homebrew, so this affects people who are not the author. No mitigation is
specified here; if reports arrive, the stroke-glyph renderer is a drop-in
alternative behind a flag.