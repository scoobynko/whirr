# whirr visual refresh — design

Date: 2026-07-17
Status: approved

## Goal

Give the gauge row one consistent visual rhythm (big hero number + detail below
on every card), replace the block-digit font with a more distinctive one, make
the fan illustration richer, and give the header breathing room — while keeping
whirr usable at small terminal sizes via the existing responsive philosophy.

## Decisions (validated with user)

1. **Font**: solid blocks stay, but letterforms change to a **4-row
   tall-rounded** style (rounded shoulders, e.g. `▄▀▀▄` tops / `▀▄▄▀` bottoms).
2. **CPU card**: hero `NN%` + compact per-core color strip + history chart.
   The numbered per-core cells are replaced by unlabeled colored cells —
   numbers were confusing; color carries the load, `E`/`P` labels stay.
3. **Temp card**: hero `NN.N°C` + chart. Thermometer illustration dropped.
4. **Memory card**: gets a hero too (used bytes, e.g. `24.3G`) so all four
   cards match. Colored by pressure level.
5. **Fan**: larger housed fan (~5 rows) with 8 animation frames for smoother
   rotation. Spin speed continues to scale with CPU load.
6. **Header**: full tier gets 1 blank row above and below the content.
7. **Responsiveness**: approach B — responsive tiers. Full design at
   comfortable heights; graceful degradation to today's compact layouts below.

## 1. Typography — `src/ui/font.rs`

- Replace the 3-row glyph set with a 4-row tall-rounded solid set.
- Glyph coverage: `0-9`, `.`, `°`, `C`, `W`, `%`, `G`, space. `%` and `G` are
  new (CPU and Memory heroes need them).
- `big_text()` returns 4 rows. Uniform-row-width invariant stays.
- Reference shapes (tuned during implementation, style must match):

```
▄▀▀▄ ▄█   ▄▀▀▄ ▀▀▀▄ ▄  █ █▀▀▀ ▄▀▀▄ ▀▀▀█ ▄▀▀▄ ▄▀▀▄
█  █  █     ▄▀  ▄▄▀ █▄▄█ ▀▀▀▄ █▄▄    ▄▀ ▄▀▀▄ ▀▄▄█
█  █  █    ▄▀     █    █    █ █  █  █   █  █    █
▀▄▄▀ ▄█▄  █▄▄▄ ▀▄▄▀    █ ▀▄▄▀ ▀▄▄▀  █   ▀▄▄▀ ▀▄▄▀
```

- The `whirr` wordmark in the header is redrawn in the same 4-row rounded
  style (stays a hand-tuned constant in `header.rs`, width ≤ ~26 cols).

## 2. Header — `src/ui/header.rs`

**Full tier** (given 7 rows by `mod.rs`):

- Row 0 blank, rows 1–5 content, row 6 blank.
- Content columns: logo (4 rows, vertically centered in the 5-row band) |
  housed fan (5 rows) | ambient facts, right-aligned, unchanged content.
- Star fan (revised 2026-07-17 from a housed-blade design, per user's visual
  reference): an 11×5 grid of ✳ cells forming 8 static arms radiating from
  an empty hub. Arms alternate white (TEXT) and amber; each frame the colors
  flip — visually identical to an 8-arm wheel rotating 45° per tick:

```
 ✳   ✳   ✳
   ✳ ✳ ✳
 ✳ ✳   ✳ ✳
   ✳ ✳ ✳
 ✳   ✳   ✳
```

- `App::tick_fan` advances modulo 8 (was 4); `fan_interval` is halved so a
  full revolution takes the same wall time as today — same perceived speed.
  Load-scaling of the interval is unchanged.
- `--no-fan` continues to hide the fan.

**Compact tier** (given 3 rows): today's header verbatim — 3-row logo,
3-row 4-frame fan, facts. No changes.

## 3. Gauge cards (full tier)

Gauges row grows from `Length(10)` to `Length(12)` (inner height 10). Shared
rhythm: hero number on top, detail below.

| Card | Hero | Below hero |
|------|------|-----------|
| CPU | `42%` in accent | 1-row per-core strip, then history chart |
| Temp | `62.4°C` in temp-gradient color | history chart |
| Power | `12.4 W` in accent (restyled only) | stacked cpu/gpu/ane chart, battery footer |
| Memory | `24.3G` used, in pressure color | stacked bar, legend, consolidated dim line |

Details:

- **CPU per-core strip**: one colored cell per core (`█`, background/gradient
  color = load), `E ` and `  P ` labels dim, replacing the numbered heatmap
  cells. No numbers in cells. Overflow beyond card width simply truncates
  (16 P-cores fit comfortably at typical widths).
- **CPU chart**: loses the top-right current-% label — the hero is the number.
- **Memory consolidated line**: `pressure NORMAL · swap 1.2G / 2.0G` in dim;
  the separate pressure line disappears (pressure also colors the hero).
- **Power**: layout as today, hero re-rendered in the new font (4 rows).

## 4. Responsive behavior

- `ui/mod.rs`: `full = area.height >= 30 && area.width >= 120`. Header
  `Length(full ? 7 : 3)`, gauges `Length(full ? 12 : 10)`. The width gate
  exists because hero strings need ~27 columns (`88.8°C`) and cards only
  reach that at ≥120-column terminals (4 cards × 30). Width-based drops
  (`show_power`, `show_temp`, `show_network`, `show_ports`) untouched.
- Each card self-decides from its own inner size — no tier flag threading:
  inner height ≥ 9 AND inner width ≥ 28 → hero layout; below → its current
  compact layout
  (CPU: numbered heatmap + chart; Temp: thermometer + line + chart;
  Power: 3-row… now 4-row font doesn't fit, so compact Power falls back to a
  single bold text line like Temp's compact readout; Memory: today's layout).
- Header self-decides the same way: area height ≥ 7 → housed fan + padding
  (the 1/5/1 padded split needs all 7 rows; shorter areas would clip the fan
  housing), else compact.

## 5. Testing

- `font.rs` unit tests: 4 uniform rows; every required glyph (`0-9 . ° C W % G`)
  renders without the `?` fallback.
- `header.rs` unit tests: all 8 housed-fan frames uniform width and height;
  compact frames keep their existing test.
- `app.rs`: fan tick modulo-8 test; interval test updated for halved values.
- `tests/render.rs`: existing size sweep must keep passing. Add: at ≥30-row
  sizes the hero font appears (assert a distinctive rounded-glyph row is
  present); at 80×24 all essential panels still render (pins the breakpoint).
- Manual `cargo run` visual pass at full size and at 80×24.

## Out of scope

- Any data/sampler changes; theme color changes; process/ports/network panels.
