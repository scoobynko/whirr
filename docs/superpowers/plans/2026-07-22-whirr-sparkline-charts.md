# Block-Sparkline History Charts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Braille line charts on the CPU, Temp, Power, and Network cards with solid filled block sparklines.

**Architecture:** A new `ui/spark.rs` helper wraps ratatui's `Sparkline`, tail-slicing history to the chart width so the newest sample sits at the right edge. Each card computes its own `u64`-scaled data and calls the helper; no sampler/history/theme changes. Network gains two stacked sparkline bands (download/upload); Power gains a dim cpu/gpu/ane legend line.

**Tech Stack:** Rust, ratatui 0.29 (`Sparkline`, `Layout`, `Paragraph`), TestBackend render tests.

## Global Constraints

- Brand colors only in charts: `theme::ACCENT` (teal), `theme::TEXT` (white), `theme::DIM`, `theme::gradient(_)`, `theme::temp_color(_)`. No off-brand hues.
- Repo uses compact hand-formatting; **do NOT run `cargo fmt`**. The enforced gate is `cargo clippy --all-targets -- -D warnings`.
- cargo needs PATH: prefix shell commands with `export PATH="$HOME/.cargo/bin:$PATH";`.
- `App::demo()` does NOT populate history buffers — chart tests must push samples into `app.<name>_hist` themselves.
- `History::iter()` yields oldest→newest; `History<T>` fields (`cpu_hist`, `temp_hist`, `power_hist`, `net_hist`) are `pub`.

---

### Task 1: `spark` helper module

**Files:**
- Create: `src/ui/spark.rs`
- Modify: `src/ui/mod.rs` (add `pub mod spark;`)

**Interfaces:**
- Produces: `pub fn render(f: &mut Frame, area: Rect, data: &[u64], max: u64, style: Style)` — renders the most-recent `area.width` samples of `data` as a filled block sparkline scaled to `max`.

- [ ] **Step 1: Register the module.** In `src/ui/mod.rs`, add after the other `pub mod` lines:

```rust
pub mod spark;
```

- [ ] **Step 2: Write the failing test.** Create `src/ui/spark.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::prelude::*;
    use ratatui::Terminal;

    fn draw(data: &[u64], max: u64, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::render(f, f.area(), data, max, Style::default()))
            .unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn renders_expected_block_levels() {
        // values 0..=8 scaled against max 8 fill one row as " ▁▂▃▄▅▆▇█"
        let s = draw(&[0, 1, 2, 3, 4, 5, 6, 7, 8], 8, 9, 1);
        assert_eq!(s, " ▁▂▃▄▅▆▇█");
    }

    #[test]
    fn shows_only_the_most_recent_width_samples() {
        // 100 ascending samples into a width-10 chart show the last 10
        // (samples 91..=100), newest at the right edge.
        let data: Vec<u64> = (1..=100).collect();
        let s = draw(&data, 100, 10, 1);
        assert_eq!(s.chars().last().unwrap(), '█', "newest (100) should be full at right");
        // sample 91/100 → 7/8 height → '▇'; the first 10 samples (1..=10) would
        // be near-empty, so a filled left cell proves we tail-sliced.
        assert!("▅▆▇█".contains(s.chars().next().unwrap()), "left cell should be the recent tail, not the oldest samples");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p whirr --lib ui::spark 2>&1 | tail -20`
Expected: FAIL — `cannot find function 'render' in module 'super'`.

- [ ] **Step 4: Write the implementation.** Prepend to `src/ui/spark.rs` (above the test module):

```rust
use ratatui::prelude::*;
use ratatui::widgets::Sparkline;

/// Filled block sparkline (`▁▂▃▄▅▆▇█`). `data` is oldest→newest; only the most
/// recent `area.width` samples are drawn so the newest lands at the right edge.
/// ratatui renders the first `min(width, len)` bars left-to-right and drops the
/// rest, so the raw oldest-first history must be tail-sliced here.
pub fn render(f: &mut Frame, area: Rect, data: &[u64], max: u64, style: Style) {
    let w = area.width as usize;
    let tail = if data.len() > w { &data[data.len() - w..] } else { data };
    let spark = Sparkline::default().data(tail).max(max.max(1)).style(style);
    f.render_widget(spark, area);
}
```

- [ ] **Step 5: Run tests to verify they pass.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p whirr --lib ui::spark 2>&1 | tail -20`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit.**

```bash
git add src/ui/spark.rs src/ui/mod.rs
git commit -m "feat: block-sparkline chart helper"
```

---

### Task 2: CPU history → sparkline

**Files:**
- Modify: `src/ui/cpu.rs` (`render_history`, imports)

**Interfaces:**
- Consumes: `spark::render` (Task 1).

- [ ] **Step 1: Replace `render_history`.** In `src/ui/cpu.rs`, replace the whole `render_history` function with:

```rust
fn render_history(f: &mut Frame, area: Rect, app: &App) {
    let data: Vec<u64> = app.cpu_hist.iter().map(|v| v.round() as u64).collect();
    super::spark::render(f, area, &data, 100, Style::default().fg(theme::ACCENT));
}
```

- [ ] **Step 2: Prune now-unused imports.** In `src/ui/cpu.rs` line 2-3, remove the chart imports. Change:

```rust
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Wrap};
```
to:
```rust
use ratatui::widgets::{Paragraph, Wrap};
```

- [ ] **Step 3: Add a sparkline assertion to the existing test.** In `src/ui/cpu.rs` `tests`, add a new test after `compact_keeps_numbered_heatmap`:

```rust
#[test]
fn history_renders_block_sparkline() {
    let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
    let mut app = App::demo();
    app.statics.e_cores = 2;
    for v in [10.0_f32, 40.0, 90.0, 60.0, 30.0] {
        app.cpu_hist.push(v);
    }
    t.draw(|f| super::render(f, f.area(), &app)).unwrap();
    let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
    assert!(s.contains('█') || s.contains('▇'), "cpu history should render filled block bars");
}
```

- [ ] **Step 4: Run tests + clippy.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p whirr --lib ui::cpu 2>&1 | tail -20 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5`
Expected: cpu tests PASS (3), clippy clean (no warnings).

- [ ] **Step 5: Commit.**

```bash
git add src/ui/cpu.rs
git commit -m "feat: CPU history as block sparkline"
```

---

### Task 3: Temp history → sparkline (baseline-shifted)

**Files:**
- Modify: `src/ui/temp.rs` (`render_chart`, imports)

**Interfaces:**
- Consumes: `spark::render` (Task 1).

- [ ] **Step 1: Replace `render_chart`.** In `src/ui/temp.rs`, replace the whole `render_chart` function with:

```rust
fn render_chart(f: &mut Frame, area: Rect, app: &App, color: Color) {
    // Baseline-shift 30→105 °C onto 0→75 so the idle-to-hot band uses the full
    // bar height instead of hugging the top.
    let data: Vec<u64> = app
        .temp_hist
        .iter()
        .map(|v| (v - 30.0).clamp(0.0, 75.0).round() as u64)
        .collect();
    super::spark::render(f, area, &data, 75, Style::default().fg(color));
}
```

- [ ] **Step 2: Prune now-unused imports.** In `src/ui/temp.rs` lines 2-3, change:

```rust
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};
```
to:
```rust
use ratatui::widgets::Paragraph;
```

- [ ] **Step 3: Add a sparkline assertion test.** In `src/ui/temp.rs` `tests`, add after `hero_falls_back_to_coarse_when_precise_would_overflow`:

```rust
#[test]
fn history_renders_block_sparkline() {
    let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
    let mut app = App::demo();
    for v in [40.0_f32, 60.0, 95.0, 70.0, 50.0] {
        app.temp_hist.push(v);
    }
    t.draw(|f| super::render(f, f.area(), &app)).unwrap();
    let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
    assert!(s.contains('█') || s.contains('▇'), "temp history should render filled block bars");
}
```

- [ ] **Step 4: Run tests + clippy.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p whirr --lib ui::temp 2>&1 | tail -20 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5`
Expected: temp tests PASS (4), clippy clean.

- [ ] **Step 5: Commit.**

```bash
git add src/ui/temp.rs
git commit -m "feat: Temp history as baseline-shifted block sparkline"
```

---

### Task 4: Power stack → total sparkline + legend

**Files:**
- Modify: `src/ui/power.rs` (`render`, `render_stack`→`render_spark`, imports)

**Interfaces:**
- Consumes: `spark::render` (Task 1).

- [ ] **Step 1: Rework the row layout and body in `render`.** In `src/ui/power.rs`, replace the `rows` layout and the `match &m.power { Some(p) => {...} ... }` block. Change the layout from:

```rust
    let rows = Layout::vertical([
        Constraint::Length(if hero { 4 } else { 1 }),
        Constraint::Min(2),    // stacked chart
        Constraint::Length(1), // battery footer
    ])
    .split(inner);
```
to:
```rust
    let rows = Layout::vertical([
        Constraint::Length(if hero { 4 } else { 1 }), // hero
        Constraint::Length(1),                         // cpu/gpu/ane legend
        Constraint::Min(2),                            // total sparkline
        Constraint::Length(1),                         // battery footer
    ])
    .split(inner);
```

Then in the `Some(p) =>` arm, after the hero is rendered (the `if hero {...} else {...}` block), replace `render_stack(f, rows[1], app);` with:

```rust
            let legend = format!("cpu {:.1} · gpu {:.1} · ane {:.1}", p.cpu_w, p.gpu_w, p.ane_w);
            f.render_widget(
                Paragraph::new(legend).style(Style::default().fg(theme::DIM)),
                rows[1],
            );
            render_spark(f, rows[2], app);
```

- [ ] **Step 2: Update the battery row index.** Still in `render`, change the final battery render target from `rows[2]` to `rows[3]`:

```rust
    f.render_widget(
        Paragraph::new(battery_line).style(Style::default().fg(theme::DIM)),
        rows[3],
    );
```

- [ ] **Step 3: Replace `render_stack` with `render_spark`.** Replace the entire `render_stack` function (and its doc comment) with:

```rust
/// Filled block sparkline of total watts (cpu + gpu + ane).
fn render_spark(f: &mut Frame, area: Rect, app: &App) {
    let data: Vec<u64> = app
        .power_hist
        .iter()
        .map(|(c, g, a)| ((c + g + a) * 10.0).round() as u64)
        .collect();
    if data.is_empty() {
        return;
    }
    let peak = app.power_hist.iter().map(|(c, g, a)| c + g + a).fold(1.0, f64::max) * 1.2;
    let max = (peak * 10.0).round() as u64;
    super::spark::render(f, area, &data, max, Style::default().fg(theme::ACCENT));
}
```

- [ ] **Step 4: Prune now-unused imports.** In `src/ui/power.rs` lines 2-3, change:

```rust
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};
```
to:
```rust
use ratatui::widgets::Paragraph;
```

- [ ] **Step 5: Extend the existing test with legend + sparkline assertions.** In `src/ui/power.rs` `tests`, replace the `hero_when_room_compact_when_small` body's assertions by adding two lines before the compact section, and add a new test. First, inside `hero_when_room_compact_when_small`, after the existing `assert!(full.contains("▀▀▀█"), ...)` line add:

```rust
        assert!(full.contains("cpu 6.4"), "power legend (cpu/gpu/ane) missing");
```

Then add a new test:

```rust
    #[test]
    fn history_renders_block_sparkline() {
        let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
        let mut app = App::demo();
        for v in [(2.0_f64, 0.5, 0.1), (5.0, 1.0, 0.2), (8.0, 2.0, 0.3)] {
            app.power_hist.push(v);
        }
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains('█') || s.contains('▇'), "power history should render filled block bars");
    }
```

- [ ] **Step 6: Run tests + clippy.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p whirr --lib ui::power 2>&1 | tail -20 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5`
Expected: power tests PASS (2), clippy clean.

- [ ] **Step 7: Commit.**

```bash
git add src/ui/power.rs
git commit -m "feat: Power total as block sparkline with cpu/gpu/ane legend"
```

---

### Task 5: Network → two stacked sparkline bands

**Files:**
- Modify: `src/ui/network.rs` (chart body, new `render_band`, imports)

**Interfaces:**
- Consumes: `spark::render` (Task 1).

- [ ] **Step 1: Replace the chart body.** In `src/ui/network.rs`, replace everything from `let down: Vec<(f64, f64)> =` through the final `f.render_widget(chart, rows[1]);` with:

```rust
    // Shared peak so the two bands' heights are directly comparable.
    let peak = app
        .net_hist
        .iter()
        .map(|(rx, tx)| rx.max(tx))
        .fold(1024.0, f64::max) // ≥1 KB/s so an idle machine doesn't render noise
        * 1.2;
    let max = peak as u64;
    let down: Vec<u64> = app.net_hist.iter().map(|(rx, _)| rx as u64).collect();
    let up: Vec<u64> = app.net_hist.iter().map(|(_, tx)| tx as u64).collect();

    let bands = Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(rows[1]);
    render_band(f, bands[0], "▼", &down, max, theme::ACCENT);
    render_band(f, bands[1], "▲", &up, max, theme::gradient(0.55));
}

/// One labeled sparkline band: a 2-col dim marker gutter then the sparkline.
fn render_band(f: &mut Frame, area: Rect, marker: &str, data: &[u64], max: u64, color: Color) {
    let cols = Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).split(area);
    f.render_widget(
        Paragraph::new(marker).style(Style::default().fg(theme::DIM)),
        cols[0],
    );
    super::spark::render(f, cols[1], data, max, Style::default().fg(color));
}
```

- [ ] **Step 2: Prune now-unused imports.** In `src/ui/network.rs` lines 2-3, change:

```rust
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};
```
to:
```rust
use ratatui::widgets::Paragraph;
```

- [ ] **Step 3: Write the failing test.** In `src/ui/network.rs`, add a `tests` module at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw() -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let mut app = App::demo();
        // download clearly larger than upload so the two bands differ.
        for i in 0..30 {
            app.net_hist.push((i as f64 * 4000.0, i as f64 * 800.0));
        }
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()
            })
            .collect()
    }

    #[test]
    fn renders_two_labeled_sparkline_bands() {
        let lines = draw();
        let joined = lines.join("\n");
        assert!(joined.contains('▼'), "download marker missing");
        assert!(joined.contains('▲'), "upload marker missing");
        assert!(
            joined.contains('█') || joined.contains('▇') || joined.contains('▆'),
            "no filled sparkline bars rendered"
        );
        // The download band (bigger data) must not be identical to the upload band.
        let down_band = lines.iter().find(|l| l.contains('▼')).unwrap();
        let up_band = lines.iter().find(|l| l.contains('▲')).unwrap();
        assert_ne!(down_band, up_band, "download and upload bands should differ");
    }
}
```

- [ ] **Step 4: Run the test to verify it passes.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p whirr --lib ui::network 2>&1 | tail -20`
Expected: PASS (1 test). (If the marker rows collapse at height 8, the two bands are `Ratio(1,2)` of a 6-row inner area = 3 rows each; markers render on each band's top row.)

- [ ] **Step 5: Run clippy.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo clippy --all-targets -- -D warnings 2>&1 | tail -5`
Expected: clean.

- [ ] **Step 6: Commit.**

```bash
git add src/ui/network.rs
git commit -m "feat: Network as two stacked block-sparkline bands"
```

---

### Task 6: Doc amendment + full verification

**Files:**
- Modify: `docs/superpowers/specs/2026-07-17-whirr-visual-refresh-design.md`

- [ ] **Step 1: Amend the visual-refresh design doc.** In `docs/superpowers/specs/2026-07-17-whirr-visual-refresh-design.md`, under the "## 3. Gauge cards (full tier)" section, add a bullet at the end of the "Details:" list:

```markdown
- **History charts**: all cards (and Network) render history as filled block
  sparklines (`▁▂▃▄▅▆▇█`), not Braille lines — see
  `2026-07-22-whirr-sparkline-charts-design.md`. Network splits into two
  stacked download/upload bands.
```

- [ ] **Step 2: Run the full test suite.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test 2>&1 | tail -30`
Expected: all lib + integration tests PASS. No test should reference Braille chart markers (`⠊`, `⢀`, etc.); if `tests/render.rs` fails, inspect — the size sweep asserts panels render, not marker glyphs.

- [ ] **Step 3: Run clippy on everything.**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo clippy --all-targets -- -D warnings 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 4: Manual visual check (human).** `cargo run` at ≥120×30 (full tier — hero + sparklines + two network bands) and at 80×24 (compact tier — sparklines still render). Confirm the sparklines read as solid filled areas and network shows separate download/upload rows.

- [ ] **Step 5: Commit.**

```bash
git add docs/superpowers/specs/2026-07-17-whirr-visual-refresh-design.md
git commit -m "docs: note sparkline charts in visual-refresh design"
```

---

## Self-Review Notes

- **Spec coverage:** helper (Task 1), CPU (2), Temp (3), Power+legend (4), Network two bands (5), doc amendment + verify (6) — all spec sections mapped.
- **Type consistency:** `spark::render(f, area, &[u64], u64, Style)` used identically in Tasks 2-5. `power_hist` tuples destructured `(c, g, a)`, `net_hist` as `(rx, tx)` — matching `app.rs` field types `History<(f64, f64, f64)>` and `History<(f64, f64)>`.
- **Import pruning:** every card that drops `Chart`/`Dataset`/`GraphType`/`Marker`/`Axis` keeps `Paragraph` (all still use it); cpu also keeps `Wrap`. clippy `-D warnings` catches any stragglers.
