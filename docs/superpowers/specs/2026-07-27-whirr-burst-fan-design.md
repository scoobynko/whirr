# whirr burst fan — design

Date: 2026-07-27
Status: approved

## Goal

Replace the header's rotating asterisk fan with a **dashed radial burst**
ported from a reference animation (`~/Downloads/whirr-anim.mp4`), rendered
with braille sub-cell hairlines and coverage-based anti-aliasing.

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
2. **Beat is wall-clock.** The solid-star flash runs on a fixed 2.0 s real-time
   period, independent of load — matching the reference exactly. This is the
   one property deliberately *not* thermal.
3. **Braille rasterization + anti-aliasing.** Sub-cell hairlines at true ray
   angles, not directional stroke glyphs. Accepted cost: appearance varies with
   terminal font.
4. **Full 9-row header band, no padding.** Height is the only lever on burst
   size (see §2); this is the largest burst achievable at zero layout cost.
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

- Fan occupies the **entire existing 9-row header**; the current 1-row top pad
  and trailing row are dropped. Header height stays 9 — no change to
  `ui/mod.rs`, no rows taken from the body at the 30-row tier threshold.
- Fan column widens 19 → **21 cells**. Canvas is 42 × 36 dots.
- Burst radius **17 dots** (31% larger than today's fan).
- Braille dots are approximately square, so the burst rasterizes as a true
  circle with no aspect correction — unlike the current cell-grid fan, which
  needs a 2.2× column stretch.
- **10 rays** at `rot + 18° + 36°k`.
- **Hub**: no ink inside 26% of radius (~4.4 dots).
- **Hairline thickness**: half-thickness 0.75 dots, measured as perpendicular
  distance to the ray, so rays stay one dot wide at every radius.

Logo and facts columns are unchanged and centre vertically against the 9-row
band.

## 3. Dash law

With `f` = normalised radius and `phase` from the beat clock, a point is lit
when:

```
f >= HUB  &&  f <= 1.0  &&  ((f - HUB - phase) / L) mod 1 < DUTY
```

`L ≈ 0.74`, `DUTY ≈ 0.86` give **one gap per ray that travels outward**; when
it slides off the rim the whole star reads solid, then a new gap emerges from
the hub. `L` and `DUTY` are tuning constants — the law is what is fixed.

Rendered across a beat cycle (21×9 preview, braille):

```
most broken                          solid moment

⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣶⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀        ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣶⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠘⣦⠀⠀⠀⣿⠀⠀⠀⣴⠃⠀⠀⠀⠀⠀        ⠀⠀⠀⠀⠀⠘⠦⠀⠀⠀⣭⠀⠀⠀⠴⠃⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠈⢳⡄⠀⠛⠀⢠⡞⠁⠀⠀⠀⠀⠀⠀        ⠀⠀⠀⠀⠀⠀⠀⢳⡄⠀⣿⠀⢠⡞⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠘⠳⠶⢤⣄⠀⠰⠂⠛⠐⠆⠀⣠⡤⠶⠞⠃⠀⠀        ⠀⠀⠘⠳⠀⢤⣄⣀⠹⠂⠛⠐⠏⣀⣠⡤⠀⠞⠃⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⢈⡁⠀⠀⠀⢈⡁⠀⠀⠀⠀⠀⠀        ⠀⠀⠀⠀⠀⠀⠀⣉⡁⠀⠀⠀⢈⣉⠀⠀⠀⠀⠀⠀⠀
⠀⠀⢠⡴⠶⠚⠋⠀⠰⠄⣤⠠⠆⠀⠙⠓⠶⢦⡄⠀⠀        ⠀⠀⢠⡴⠀⠚⠋⠉⣰⠄⣤⠠⣆⠉⠙⠓⠀⢦⡄⠀⠀
⠀⠀⠀⠀⠀⠀⢀⡼⠃⠀⣤⠀⠘⢧⡀⠀⠀⠀⠀⠀⠀        ⠀⠀⠀⠀⠀⠀⠀⡼⠃⠀⣿⠀⠘⢧⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⢠⠟⠀⠀⠀⣿⠀⠀⠀⠻⡄⠀⠀⠀⠀⠀⠀        ⠀⠀⠀⠀⠀⢠⠖⠀⠀⠀⣛⠀⠀⠀⠲⡄⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀        ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
```

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

Two independent clocks:

- **Rotation — thermal.** Angular velocity ramps `360°/14s` idle → `360°/2s`
  hot. The heat term currently inlined in `fan_interval()` (temperature 55→95°C,
  falling back to CPU load when no sensor reads) is factored out into its own
  `heat()` helper, since both the frame interval and ω now consume it. Rotation
  is integrated against real elapsed time rather than counted in fixed steps.
- **Beat — wall clock.** `phase = fract(elapsed / 2.0s)`, load-independent.

**Frame interval scales 125 ms idle → 60 ms hot.** This is forced, not
cosmetic: 10-fold symmetry means any per-frame rotation above 18° aliases and
the star appears to spin backwards. At a fixed 8 fps, the hot end (2 s/rev)
would need 45°/frame. The scaled interval gives 3.2°/frame idle and
10.8°/frame hot, both safely under the limit.

Cost: worst case 16.7 fps × 151 µs per full 160×44 redraw ≈ **0.25% of one
core** (measured). The terminal receives only the changed fan cells.

## 7. Code changes

| File | Change |
|---|---|
| `src/app.rs` | Replace `fan_frame: usize` with `fan_angle_deg: f32` and `beat_phase: f32`. `tick_fan(dt: Duration)` integrates ω(heat) and wraps the angle. `fan_interval()` returns the frame interval (125→60 ms). |
| `src/main.rs` | Pass real elapsed `dt` to `tick_fan`; set `beat_phase` from a wall-clock `Instant` held in the loop. |
| `src/ui/header.rs` | New braille rasterizer replacing `render_star_fan`. Band 7→9 rows, pads dropped, fan column 19→21. |
| `src/ui/theme.rs` | Blend helper for coverage dimming. |
| `tests/render.rs` | Replace the three `✳` tier assertions with a braille marker check. |

Both phases live as plain `App` fields rather than being read from the clock
inside render, so rendering stays pure and every test is deterministic.

Unchanged: `--no-fan`, and the compact-tier 4-frame ASCII fan below the full
tier — its frame derives from the angle as `(angle / 90) % 4`.

## 8. Tests

- **Ray geometry**: at a solid beat phase, 10 distinct ray directions with 36°
  spacing.
- **Rotation**: renders differ between angles; sweeping 0→360° never drops
  below a minimum ink floor (catches strobing and dropout).
- **Beat**: across one 2.0 s cycle a solid frame and a maximally-broken frame
  both exist; ink stays within bounds and never reaches zero.
- **Two-tone**: only TEXT and ACCENT appear as base tones, and both are
  present.
- **Anti-aliasing**: every lit cell's color lies on the blend line between
  `BG_CELL` and its ray tone.
- **Tier**: braille renders at ≥120×30 and never at 80×24 or 119×30.

## 9. Open risk

**Braille rendering across terminal fonts** cannot be de-risked in tests — dot
weight, size, and cell advance vary. This needs a visual check in the real
terminal early in implementation, not at the end. If braille proves unusable,
the fallback is directional stroke glyphs (`─ ╲ │ ╱` chosen from each ray's
on-screen angle), which renders identically everywhere but quantises rays to
four angles.
