# whirr sparkline charts — design

Date: 2026-07-22
Status: approved

## Goal

Replace the thin Braille line charts on the gauge and network cards with solid
filled **block sparklines** (`▁▂▃▄▅▆▇█`). Braille lines read as wispy and jagged
at these small heights; solid bars match the solid-block direction of the rest
of the visual refresh (hero font, rotating asterisk fan) and read as a filled
area chart. Network additionally splits its mirrored up/down line pair into two
stacked, separately-labeled sparkline rows.

## Decisions (validated with user)

1. **Chart style**: block sparkline (filled bars) on CPU, Temp, Power, Network.
2. **Power**: a single filled sparkline of **total** watts, plus a dim
   `cpu X · gpu Y · ane Z` legend line so the cpu/gpu/ane breakdown the old
   3-line stack conveyed stays visible.
3. **Network**: two stacked sparkline rows sharing one peak scale — download on
   top, upload below — each with a dim `▼`/`▲` gutter.

## 1. Shared helper — `src/ui/spark.rs` (new)

```
pub fn render(f: &mut Frame, area: Rect, data: &[u64], max: u64, style: Style)
```

- Selects the **tail** of `data` sized to `area.width` (drops oldest samples so
  the newest sample lands at the right edge — ratatui's `Sparkline` renders the
  first `min(width, len)` bars left-to-right and truncates the rest, so the raw
  oldest-first history must be tail-sliced).
- Builds `Sparkline::default().data(tail).max(max).style(style)` and renders it.
  Multi-row areas give tall bars (8 levels per cell row) so the chart reads as a
  filled area, not a single-row strip.
- All value→`u64` scaling stays in the calling card; the helper is pure
  rendering. Registered in `ui/mod.rs` as `pub mod spark;`.

## 2. Per-card changes

Both tiers change — compact cards draw charts too, so each card's chart renderer
switches once and serves both layouts.

| Card | Data (→ u64) | max | Color |
|------|--------------|-----|-------|
| CPU | `cpu%` rounded | `100` | `ACCENT` |
| Temp | `(t − 30).clamp(0, 75)` rounded | `75` | `temp_color(t)` |
| Power | `total_w × 10` rounded | `peak × 1.2 × 10` | `ACCENT` |
| Network down | `rx_rate` bytes/s | shared peak | `ACCENT` |
| Network up | `tx_rate` bytes/s | shared peak | `gradient(0.55)` |

- **CPU** (`cpu.rs`): `render_history` swaps its `Chart`/`Dataset` for
  `spark::render`. The compact tier keeps its top-right `NN%` overlay (drawn
  after the sparkline on chart row 0).
- **Temp** (`temp.rs`): `render_chart` swaps to `spark::render`. Baseline shift
  (`−30`, span `75`) matches today's `[30, 105]` y-axis so idle-to-hot temps use
  the full bar height instead of hugging the top. Both hero and compact tiers
  call `render_chart`, so both update.
- **Power** (`power.rs`): `render_stack` becomes a single total-watt sparkline.
  A dim legend line `cpu {:.1} · gpu {:.1} · ane {:.1}` (latest sample) is added
  as `Length(1)` directly under the hero. New row layout:
  `[hero, Length(1) legend, Min(2) sparkline, Length(1) battery]`.
- **Network** (`network.rs`): header line unchanged. The chart area splits into
  two equal vertical bands; each band splits horizontally into a `Length(2)` dim
  `▼`/`▲` marker gutter and the sparkline. Both sparklines use the same peak
  (`max(rx, tx)` over the window, floored at `1024`, ×1.2) so their heights are
  comparable. The mirrored negative-`tx` line and the two-`Dataset` `Chart` are
  removed.

## 3. Testing

- `spark.rs` unit tests: known data renders the expected block glyphs (mirror
  ratatui's own `[0..8] → " ▁▂▃▄▅▆▇█"`); tail-selection — 100 ascending samples
  into a width-10 area shows the last 10 (newest at the right edge).
- `network.rs` unit test (new): at a full size, the card renders block-bar
  glyphs and the two bands differ (download row ≠ upload row); `▼` and `▲`
  markers both present.
- Existing CPU/Temp/Power tests (hero glyphs, thermometer, coarse-fallback) are
  unaffected — heroes and layout tiers don't change, only the chart body.
- `tests/render.rs` size sweep must keep passing; no assertion there targets
  Braille markers, but verify none regress.

## Out of scope

- Sampler, `History`, or theme-color changes.
- Hero numbers, fonts, fan, header, or responsive tier gates.
- Reading real SMC fan RPM (tracked separately).

## Amendment to the visual-refresh design

`docs/superpowers/specs/2026-07-17-whirr-visual-refresh-design.md` gets a one-line
note under the gauge-cards section: history charts render as block sparklines
(this doc), not Braille lines.
