# whirr Visual Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hero numbers on all four gauge cards in a new 4-row rounded solid font, a housed 8-frame fan, a padded header, and responsive fallback to today's compact layouts on small terminals.

**Architecture:** `ui/font.rs` gains a 4-row glyph set plus a `hero_fits(Rect)` predicate; each gauge card branches on `hero_fits(inner)` between a new hero layout and its existing compact layout; `ui/mod.rs` picks row heights from one `full` tier flag (`height ≥ 30 && width ≥ 120`). The fan gains 8 blade frames inside a fixed housing; `App` ticks frames modulo 8 at half the old interval so perceived speed is unchanged.

**Tech Stack:** Rust, ratatui (TestBackend for render tests), cargo.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-17-whirr-visual-refresh-design.md`.
- Font style: solid blocks, 4 rows, rounded shoulders (`▄▀▀▄` tops, `▀▄▄▀` bottoms).
- Full tier: `area.height >= 30 && area.width >= 120` (mod.rs); per-card hero: `inner.height >= 9 && inner.width >= 28`.
- Compact tier renders exactly today's layouts (only exception: Power's compact hero becomes one bold text line, since the 3-row font is deleted).
- No data/sampler/theme-color changes. Panels other than header + 4 gauge cards untouched.
- Every task: run `cargo test` before committing; test code lives with the module it tests.

---

### Task 1: 4-row tall-rounded font (`src/ui/font.rs`)

**Files:**
- Modify: `src/ui/font.rs` (full rewrite)

**Interfaces:**
- Produces: `font::big_text(&str) -> Vec<String>` — now returns **4** rows (was 3). `font::hero_fits(inner: Rect) -> bool` — true when a hero layout fits (`height >= 9 && width >= 28`). Glyphs cover `0-9 . ° C W % G` and space.
- Note: `power.rs` renders `big_text` into a 3-row area until Task 4 — it clips harmlessly in the interim; render tests only assert non-panic.

- [ ] **Step 1: Write the failing tests**

Replace the `tests` module at the bottom of `src/ui/font.rs` with:

```rust
#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    #[test]
    fn four_uniform_rows() {
        let rows = super::big_text("42.0 W");
        assert_eq!(rows.len(), 4);
        let w = rows[0].chars().count();
        assert!(rows.iter().all(|r| r.chars().count() == w));
    }

    #[test]
    fn all_required_glyphs_exist() {
        for c in "0123456789.°CW%G ".chars() {
            let g = super::glyph(c);
            assert_eq!(g.len(), 4, "glyph {c:?} must have 4 rows");
            let w = g[0].chars().count();
            assert!(g.iter().all(|r| r.chars().count() == w), "glyph {c:?} rows uneven");
            assert_ne!(g, super::glyph('\u{1}'), "glyph {c:?} falls back to '?'");
        }
    }

    #[test]
    fn hero_fits_thresholds() {
        let r = |w, h| Rect::new(0, 0, w, h);
        assert!(super::hero_fits(r(28, 9)));
        assert!(!super::hero_fits(r(28, 8)));
        assert!(!super::hero_fits(r(27, 9)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::font -- --nocapture`
Expected: FAIL — `four_uniform_rows` (3 rows ≠ 4), `all_required_glyphs_exist` (row counts), `hero_fits_thresholds` (function not found: compile error first; that counts as the failing state).

- [ ] **Step 3: Write the implementation**

Replace everything above the tests module in `src/ui/font.rs` with:

```rust
use ratatui::layout::Rect;

/// 4-row tall-rounded solid glyphs. Rounded shoulders come from `▄▀▀▄`-style
/// tops and `▀▄▄▀` bottoms; every row of a glyph has equal width.
fn glyph(c: char) -> [&'static str; 4] {
    match c {
        '0' => ["▄▀▀▄", "█  █", "█  █", "▀▄▄▀"],
        '1' => ["▄█ ", " █ ", " █ ", "▄█▄"],
        '2' => ["▄▀▀▄", "  ▄▀", " ▄▀ ", "█▄▄▄"],
        '3' => ["▄▀▀▄", " ▄▄▀", "   █", "▀▄▄▀"],
        '4' => ["▄  █", "█  █", "▀▀▀█", "   █"],
        '5' => ["█▀▀▀", "▀▀▀▄", "   █", "▀▄▄▀"],
        '6' => ["▄▀▀▄", "█▄▄ ", "█  █", "▀▄▄▀"],
        '7' => ["▀▀▀█", "  ▄▀", " █  ", " █  "],
        '8' => ["▄▀▀▄", "▀▄▄▀", "█  █", "▀▄▄▀"],
        '9' => ["▄▀▀▄", "█  █", " ▀▀█", "▀▄▄▀"],
        '.' => ["  ", "  ", "  ", "▄ "],
        '°' => ["▄▀▄", "▀▄▀", "   ", "   "],
        'C' => ["▄▀▀▄", "█   ", "█   ", "▀▄▄▀"],
        'W' => ["█   █", "█   █", "█ ▄ █", "▀▄▀▄▀"],
        '%' => ["█  ▄▀", "  ▄▀ ", " ▄▀  ", "▄▀  █"],
        'G' => ["▄▀▀▄", "█   ", "█ ▀█", "▀▄▄▀"],
        ' ' => [" ", " ", " ", " "],
        _ => ["?", "?", "?", "?"],
    }
}

pub fn big_text(s: &str) -> Vec<String> {
    let mut rows = vec![String::new(); 4];
    for c in s.chars() {
        let g = glyph(c);
        for (i, row) in rows.iter_mut().enumerate() {
            row.push_str(g[i]);
            row.push(' ');
        }
    }
    rows
}

/// Whether a card's inner area has room for a 4-row hero layout: 4 hero rows
/// + at least a strip/chart below (height 9) and the widest hero string
/// (`88.8°C` ≈ 27 cols) plus margin.
pub fn hero_fits(inner: Rect) -> bool {
    inner.height >= 9 && inner.width >= 28
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ui::font`
Expected: PASS (3 tests). Also run `cargo test` — the full suite must stay green (render sweep only asserts non-panic/needles; power clips its now-4-row hero into 3 rows without failing).

- [ ] **Step 5: Commit**

```bash
git add src/ui/font.rs
git commit -m "feat: 4-row tall-rounded hero font with hero_fits predicate"
```

---

### Task 2: Fan timing — 8 frames at half interval (`src/app.rs`)

**Files:**
- Modify: `src/app.rs:224-232` (`fan_interval`, `tick_fan`) and the `fan_speed_scales_with_load` test

**Interfaces:**
- Produces: `app.fan_frame` now cycles `0..8`. `fan_interval()` returns 250ms idle → 50ms at full load. Consumers (Task 3): full header uses `fan_frame % 8`, compact uses `(fan_frame / 2) % 4` to keep its perceived speed.

- [ ] **Step 1: Update/add the failing tests**

In `src/app.rs` tests module, change `fan_speed_scales_with_load`'s final assert and add a wrap test:

```rust
    #[test]
    fn fan_speed_scales_with_load() {
        let mut a = App::new(false);
        let idle = a.fan_interval();
        let mut f = demo_fast();
        f.total_cpu = 100.0;
        a.ingest(Snapshot::Fast(f));
        assert!(a.fan_interval() < idle);
        assert_eq!(idle.as_millis(), 250);
        assert_eq!(a.fan_interval().as_millis(), 50);
    }

    #[test]
    fn fan_frame_wraps_at_eight() {
        let mut a = App::new(false);
        for _ in 0..7 {
            a.tick_fan();
        }
        assert_eq!(a.fan_frame, 7);
        a.tick_fan();
        assert_eq!(a.fan_frame, 0);
    }
```

(Note: `fan_speed_scales_with_load` currently builds its snap via `app_with_procs().fast.unwrap()` — the rewrite above uses `demo_fast()` directly, same data.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib app::tests::fan`
Expected: FAIL — interval is 500/100ms, frame wraps at 4.

- [ ] **Step 3: Write the implementation**

Replace `fan_interval` and `tick_fan` in `src/app.rs`:

```rust
    pub fn fan_interval(&self) -> Duration {
        let load = self.fast.as_ref().map_or(0.0, |f| f.total_cpu / 100.0);
        // Half the 4-frame-era interval: 8 frames per revolution at twice
        // the tick rate keeps the perceived rotation speed identical.
        Duration::from_millis((250.0 - 200.0 * f64::from(load)) as u64)
    }

    pub fn tick_fan(&mut self) {
        self.fan_frame = (self.fan_frame + 1) % 8;
        self.dirty = true;
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib app`
Expected: PASS. (The header still indexes `fan_frame % 4` until Task 3 — valid indices, just a scrambled-looking cycle for one commit; render tests stay green.)

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: fan ticks 8 frames at half interval for smoother spin"
```

---

### Task 3: Header — 4-row logo, housed fan, tiers (`src/ui/header.rs`)

**Files:**
- Modify: `src/ui/header.rs` (full rewrite)

**Interfaces:**
- Consumes: `app.fan_frame` (mod 8, Task 2).
- Produces: `header::render(f, area, app)` self-selects: `area.height >= 5` → full padded layout (blank row, 5-row band, blank row), else today's compact 3-row layout. Constants `LOGO4: [&str; 4]`, `FAN_BLADES: [[&str; 3]; 8]`, compact `LOGO`/`FAN_FRAMES` kept.

- [ ] **Step 1: Write the failing tests**

Replace the tests module in `src/ui/header.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn logo_is_three_uniform_rows_within_budget() {
        let w = super::LOGO[0].chars().count();
        assert!(super::LOGO.iter().all(|r| r.chars().count() == w));
        assert!(w <= 21);
    }

    #[test]
    fn logo4_is_four_uniform_rows_within_budget() {
        let w = super::LOGO4[0].chars().count();
        assert_eq!(super::LOGO4.len(), 4);
        assert!(super::LOGO4.iter().all(|r| r.chars().count() == w));
        assert!(w <= 26);
    }

    #[test]
    fn fan_frames_are_uniform() {
        for frame in super::FAN_FRAMES {
            let w = frame[0].chars().count();
            assert!(frame.iter().all(|r| r.chars().count() == w));
        }
    }

    #[test]
    fn housed_fan_blades_are_eight_uniform_frames() {
        assert_eq!(super::FAN_BLADES.len(), 8);
        for frame in super::FAN_BLADES {
            assert_eq!(frame.len(), 3);
            assert!(frame.iter().all(|r| r.chars().count() == 5), "{frame:?}");
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::header`
Expected: compile FAIL — `LOGO4`, `FAN_BLADES` not defined.

- [ ] **Step 3: Write the implementation**

Replace `src/ui/header.rs` above the tests with:

```rust
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::App;
use crate::units::fmt_duration;

// Compact-tier wordmark (3 rows) — kept verbatim for small terminals.
const LOGO: [&str; 3] = [
    "█ █ █ █ █ █ █▀█ █▀█",
    "█ █ █ █▀█ █ █▀▄ █▀▄",
    "▀▄▀▄▀ █ █ █ █ █ █ █",
];

// Full-tier wordmark: W H I R R in the same 4-row tall-rounded style as
// the hero font (ui/font.rs).
const LOGO4: [&str; 4] = [
    "█   █ █  █ ▄█▄ █▀▀▄ █▀▀▄",
    "█   █ █▄▄█  █  █▄▄▀ █▄▄▀",
    "█ ▄ █ █  █  █  █ ▀▄ █ ▀▄",
    "▀▄▀▄▀ █  █ ▄█▄ █  █ █  █",
];

// Compact-tier 2-arm fan (4 frames) — kept verbatim.
const FAN_FRAMES: [[&str; 3]; 4] = [
    ["  │  ", "  ✻  ", "  │  "],
    ["   ╱ ", "  ✻  ", " ╱   "],
    ["     ", "──✻──", "     "],
    [" ╲   ", "  ✻  ", "   ╲ "],
];

// Full-tier blades: 8 frames — the four 2-arm positions plus blur frames
// between them, so rotation reads smooth inside the fixed housing.
const FAN_BLADES: [[&str; 3]; 8] = [
    ["  │  ", "  ✺  ", "  │  "],
    ["  │╱ ", "  ✺  ", " ╱│  "],
    ["   ╱ ", "  ✺  ", " ╱   "],
    ["   ╱ ", "──✺──", " ╱   "],
    ["     ", "──✺──", "     "],
    [" ╲   ", "──✺──", "   ╲ "],
    [" ╲   ", "  ✺  ", "   ╲ "],
    [" ╲│  ", "  ✺  ", "  │╲ "],
];

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    if area.height >= 5 {
        render_full(f, area, app);
    } else {
        render_compact(f, area, app);
    }
}

fn render_full(f: &mut Frame, area: Rect, app: &App) {
    // Breathing room: one blank row above and below the 5-row content band.
    let bands =
        Layout::vertical([Constraint::Length(1), Constraint::Length(5), Constraint::Min(0)])
            .split(area);
    let band = bands[1];
    let cols = Layout::horizontal([
        Constraint::Length(26), // logo
        Constraint::Length(11), // housed fan
        Constraint::Min(0),     // ambient facts
    ])
    .split(band);

    let logo_lines: Vec<Line> = LOGO4
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(theme::ACCENT).bold()))
        .collect();
    f.render_widget(Paragraph::new(logo_lines), cols[0]);

    if !app.no_fan {
        render_housed_fan(f, cols[1], app.fan_frame % 8);
    }

    // Facts sit one row down so their block centers against the fan hub.
    let facts_area = Rect {
        y: band.y + 1,
        height: band.height.saturating_sub(1).min(3),
        ..cols[2]
    };
    f.render_widget(facts_paragraph(app), facts_area);
}

fn render_compact(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::horizontal([
        Constraint::Length(21), // logo
        Constraint::Length(7),  // fan
        Constraint::Min(0),     // ambient facts
    ])
    .split(area);

    let logo_lines: Vec<Line> = LOGO
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(theme::ACCENT).bold()))
        .collect();
    f.render_widget(Paragraph::new(logo_lines), cols[0]);

    if !app.no_fan {
        // fan_frame advances mod 8 at double rate; halve it here so the
        // 4-frame compact fan keeps its original perceived speed.
        let frame = FAN_FRAMES[(app.fan_frame / 2) % 4];
        let fan_lines: Vec<Line> = frame
            .iter()
            .map(|l| Line::styled(*l, Style::default().fg(theme::DIM)))
            .collect();
        f.render_widget(Paragraph::new(fan_lines), cols[1]);
    }

    f.render_widget(facts_paragraph(app), cols[2]);
}

fn render_housed_fan(f: &mut Frame, area: Rect, frame: usize) {
    let dim = Style::default().fg(theme::DIM);
    let txt = Style::default().fg(theme::TEXT);
    let blades = FAN_BLADES[frame];
    let mut lines = vec![Line::styled(" ╭─────╮", dim)];
    for row in blades {
        lines.push(Line::from(vec![
            Span::styled(" │", dim),
            Span::styled(row, txt),
            Span::styled("│", dim),
        ]));
    }
    lines.push(Line::styled(" ╰─────╯", dim));
    f.render_widget(Paragraph::new(lines), area);
}

fn facts_paragraph(app: &App) -> Paragraph<'_> {
    let (uptime, load) = (
        app.medium.as_ref().map_or(0, |m| m.uptime_secs),
        app.fast.as_ref().map_or(0.0, |f| f.load_avg),
    );
    let facts = vec![
        Line::from(format!(
            "{} · macOS {}",
            app.statics.chip, app.statics.os_version
        )),
        Line::from(format!("up {} · load {:.2}", fmt_duration(uptime), load)),
        Line::from(""),
    ];
    Paragraph::new(facts)
        .style(Style::default().fg(theme::DIM))
        .alignment(Alignment::Right)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ui::header && cargo test`
Expected: PASS. Header still receives 3 rows from `mod.rs` until Task 8, so the app renders the compact tier everywhere — unchanged appearance.

- [ ] **Step 5: Commit**

```bash
git add src/ui/header.rs
git commit -m "feat: tiered header with 4-row logo and housed 8-frame fan"
```

---

### Task 4: Power card hero tiers (`src/ui/power.rs`)

**Files:**
- Modify: `src/ui/power.rs`

**Interfaces:**
- Consumes: `font::big_text` (4 rows), `font::hero_fits` (Task 1).
- Produces: full tier `[hero 4, stack chart ≥2, battery 1]`; compact tier `[one bold line, stack chart ≥2, battery 1]`. `render_stack` unchanged.

- [ ] **Step 1: Write the failing test**

Add to the bottom of `src/ui/power.rs`:

```rust
#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn hero_when_room_compact_when_small() {
        // demo power total = 6.4 + 1.2 + 0.3 = 7.9 → "7.9 W"
        let full = draw(40, 12); // inner 38x10 → hero
        assert!(full.contains("▀▀▀█"), "4-row '7' glyph missing"); // '7' row 0
        let compact = draw(40, 10); // inner 38x8 → compact
        assert!(compact.contains("7.9 W"));
        assert!(!compact.contains("▀▀▀█"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib ui::power`
Expected: FAIL — hero currently renders into a `Length(3)` row: the 4th glyph row is clipped, and no compact text line exists.

- [ ] **Step 3: Write the implementation**

Replace the `render` function in `src/ui/power.rs` (keep `render_stack` as-is):

```rust
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Power", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(m) = app.medium.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let hero = font::hero_fits(inner);
    let rows = Layout::vertical([
        Constraint::Length(if hero { 4 } else { 1 }),
        Constraint::Min(2),    // stacked chart
        Constraint::Length(1), // battery footer
    ])
    .split(inner);

    match &m.power {
        Some(p) => {
            let total = p.cpu_w + p.gpu_w + p.ane_w;
            let text = format!("{total:.1} W");
            if hero {
                let lines: Vec<Line> = font::big_text(&text)
                    .into_iter()
                    .map(|r| Line::styled(r, Style::default().fg(theme::ACCENT)))
                    .collect();
                f.render_widget(Paragraph::new(lines), rows[0]);
            } else {
                f.render_widget(
                    Paragraph::new(Span::styled(text, Style::default().fg(theme::ACCENT).bold())),
                    rows[0],
                );
            }
            render_stack(f, rows[1], app);
        }
        None => {
            f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), rows[0]);
        }
    }

    let battery_line = match &m.battery {
        Some(b) => {
            let state = if b.charging { "⚡" } else { "🔋" };
            let health = b.health_pct.map_or(String::new(), |h| format!(" · health {h}%"));
            format!("{state} {}% · {} cycles{health}", b.percent, b.cycles)
        }
        None => String::new(), // desktop Mac: hide line
    };
    f.render_widget(
        Paragraph::new(battery_line).style(Style::default().fg(theme::DIM)),
        rows[2],
    );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ui::power && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/power.rs
git commit -m "feat: power card hero in 4-row font with compact fallback"
```

---

### Task 5: CPU card hero + per-core strip (`src/ui/cpu.rs`)

**Files:**
- Modify: `src/ui/cpu.rs`

**Interfaces:**
- Consumes: `font::big_text`, `font::hero_fits`, `theme::gradient`, `app.statics.e_cores`.
- Produces: full tier `[hero 4, strip 1, chart ≥3]` — strip is one `█` cell per core colored by `gradient(load/100)`, `E`/`P` labels dim, no numbers. Compact tier: today's numbered heatmap + labeled chart, unchanged.

- [ ] **Step 1: Write the failing test**

Add to the bottom of `src/ui/cpu.rs`:

```rust
#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn hero_with_strip_when_room() {
        // demo total_cpu = 41.0 → "41%"
        let full = draw(40, 12);
        assert!(full.contains("▄  █"), "4-row '4' glyph missing");
        assert!(full.contains(" P "), "per-core strip P label missing");
        assert!(!full.contains(" 12 "), "numbered heatmap cell should be gone in hero tier");
    }

    #[test]
    fn compact_keeps_numbered_heatmap() {
        let compact = draw(40, 10);
        assert!(compact.contains(" 12"), "per-core numbered cell missing"); // demo core 0 at 12%
        assert!(!compact.contains("▄  █"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib ui::cpu`
Expected: FAIL — no hero branch exists.

- [ ] **Step 3: Write the implementation**

Replace `src/ui/cpu.rs` above the new tests with:

```rust
use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Wrap};

use super::{font, theme};
use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("CPU", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(fast) = app.fast.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    if font::hero_fits(inner) {
        let rows = Layout::vertical([
            Constraint::Length(4), // hero
            Constraint::Length(1), // per-core strip
            Constraint::Min(3),    // history chart
        ])
        .split(inner);
        let hero: Vec<Line> = font::big_text(&format!("{:.0}%", fast.total_cpu))
            .into_iter()
            .map(|r| Line::styled(r, Style::default().fg(theme::ACCENT)))
            .collect();
        f.render_widget(Paragraph::new(hero), rows[0]);
        render_core_strip(f, rows[1], app, &fast.per_core);
        render_history(f, rows[2], app);
    } else {
        let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(inner);
        render_heatmap(f, rows[0], app, &fast.per_core);
        render_history(f, rows[1], app);
        // compact tier keeps the small current-% label over the chart
        let label = Line::from(Span::styled(
            format!("{:>3.0}%", fast.total_cpu),
            Style::default().fg(theme::ACCENT).bold(),
        ))
        .right_aligned();
        f.render_widget(Paragraph::new(label), Rect { height: 1, ..rows[1] });
    }
}

/// One colored cell per core — load carried by color alone (heat strip).
fn render_core_strip(f: &mut Frame, area: Rect, app: &App, per_core: &[f32]) {
    let e = app.statics.e_cores.min(per_core.len());
    let mut spans = vec![Span::styled("E ", Style::default().fg(theme::DIM))];
    for (i, &load) in per_core.iter().enumerate() {
        if i == e {
            spans.push(Span::styled("  P ", Style::default().fg(theme::DIM)));
        }
        spans.push(Span::styled("█", Style::default().fg(theme::gradient(load / 100.0))));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_heatmap(f: &mut Frame, area: Rect, app: &App, per_core: &[f32]) {
    let e = app.statics.e_cores.min(per_core.len());
    let mut spans = vec![Span::styled("E ", Style::default().fg(theme::DIM))];
    for (i, &load) in per_core.iter().enumerate() {
        if i == e {
            spans.push(Span::styled("  P ", Style::default().fg(theme::DIM)));
        }
        spans.push(Span::styled(
            format!("{:>3}", (load as u16).min(99)),
            Style::default().fg(theme::TEXT).bg(theme::gradient(load / 100.0)),
        ));
        spans.push(Span::raw(" "));
    }
    // On machines with many cores (e.g. M5 P-cores) the cell spans overflow a
    // single line's width; wrap onto the 3-row area allotted to the heatmap
    // instead of silently truncating the trailing cores off the panel edge.
    f.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
        area,
    );
}

fn render_history(f: &mut Frame, area: Rect, app: &App) {
    let points: Vec<(f64, f64)> = app
        .cpu_hist
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, f64::from(v)))
        .collect();
    let dataset = Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme::ACCENT))
        .data(&points);
    let chart = Chart::new(vec![dataset])
        .x_axis(Axis::default().bounds([0.0, 59.0]))
        .y_axis(Axis::default().bounds([0.0, 100.0]))
        .style(Style::default().fg(theme::DIM));
    f.render_widget(chart, area);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ui::cpu && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/cpu.rs
git commit -m "feat: cpu card hero with per-core color strip"
```

---

### Task 6: Temp card hero tiers (`src/ui/temp.rs`)

**Files:**
- Modify: `src/ui/temp.rs`

**Interfaces:**
- Consumes: `font::big_text`, `font::hero_fits`, `theme::temp_color`.
- Produces: full tier `[hero 4 ("62.4°C" in temp color), chart ≥3]`, no thermometer. Compact tier: today's thermometer + readout + chart, unchanged (`fill_ratio_clamps` test kept).

- [ ] **Step 1: Write the failing test**

Add inside the existing `tests` module in `src/ui/temp.rs`:

```rust
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn hero_drops_thermometer_when_room() {
        // demo temp = 88.0 → "88.0°C"
        let full = draw(40, 12);
        assert!(full.contains("▄▀▀▄"), "4-row '8' glyph missing");
        assert!(!full.contains("▐"), "thermometer should be gone in hero tier");
        let compact = draw(40, 10);
        assert!(compact.contains("▐"), "thermometer missing in compact tier");
        assert!(compact.contains("88.0°C"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib ui::temp`
Expected: FAIL — thermometer renders at both sizes, no hero glyphs.

- [ ] **Step 3: Write the implementation**

Replace the `render` function in `src/ui/temp.rs` (keep `render_thermometer` as-is) and add `use super::font;` to the imports (`use super::{font, theme};`):

```rust
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Temp", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let temp = app.medium.as_ref().and_then(|m| m.temp_c);
    let Some(t) = temp else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };
    let color = theme::temp_color(t);

    if font::hero_fits(inner) {
        let rows = Layout::vertical([Constraint::Length(4), Constraint::Min(3)]).split(inner);
        let hero: Vec<Line> = font::big_text(&format!("{t:.1}°C"))
            .into_iter()
            .map(|r| Line::styled(r, Style::default().fg(color)))
            .collect();
        f.render_widget(Paragraph::new(hero), rows[0]);
        render_chart(f, rows[1], app, color);
    } else {
        let cols = Layout::horizontal([Constraint::Length(3), Constraint::Min(4)]).split(inner);
        render_thermometer(f, cols[0], t, color);

        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).split(cols[1]);
        f.render_widget(
            Paragraph::new(Span::styled(format!("{t:.1}°C"), Style::default().fg(color).bold())),
            rows[0],
        );
        render_chart(f, rows[1], app, color);
    }
}

fn render_chart(f: &mut Frame, area: Rect, app: &App, color: Color) {
    let points: Vec<(f64, f64)> = app
        .temp_hist
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, f64::from(v)))
        .collect();
    let chart = Chart::new(vec![Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(color))
        .data(&points)])
    .x_axis(Axis::default().bounds([0.0, 59.0]))
    .y_axis(Axis::default().bounds([30.0, 105.0]));
    f.render_widget(chart, area);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ui::temp && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/temp.rs
git commit -m "feat: temp card hero replaces thermometer at full size"
```

---

### Task 7: Memory card hero tiers (`src/ui/memory.rs`)

**Files:**
- Modify: `src/ui/memory.rs`

**Interfaces:**
- Consumes: `font::big_text`, `font::hero_fits`, `theme::pressure_color`.
- Produces: full tier `[hero 4 (used GiB, e.g. "6.5G", in pressure color), spacer 1, bar 1, legend 1, info 1]` where info = `pressure NORMAL · swap 0 B / 1.0 GB`. Compact tier: today's 4-line layout, unchanged. `segment_widths` unchanged.

- [ ] **Step 1: Write the failing test**

Add inside the existing `tests` module in `src/ui/memory.rs`:

```rust
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn hero_shows_used_gib_when_room() {
        // demo used = 4G + 2G + 1G = 7_000_000_000 B = 6.5 GiB → "6.5G"
        let full = draw(40, 12);
        assert!(full.contains("█▄▄ "), "4-row '6' glyph missing"); // '6' row 1
        assert!(full.contains("pressure NORMAL · swap"), "consolidated info line missing");
        let compact = draw(40, 10);
        assert!(!compact.contains("█▄▄ "));
        assert!(compact.contains("pressure "));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib ui::memory`
Expected: FAIL — no hero branch, no consolidated line.

- [ ] **Step 3: Write the implementation**

In `src/ui/memory.rs`, change the imports to include font (`use super::{font, theme};`) and replace the `render` function:

```rust
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Memory", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(mem) = app.medium.as_ref().and_then(|m| m.memory.as_ref()) else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let state = match mem.pressure {
        PressureLevel::Normal => "NORMAL",
        PressureLevel::Warn => "WARN",
        PressureLevel::Critical => "CRITICAL",
    };
    let pcolor = theme::pressure_color(mem.pressure);

    let parts = [mem.app, mem.wired, mem.compressed, mem.free];
    let colors = [theme::ACCENT, theme::gradient(0.6), theme::AMBER, theme::BG_CELL];
    let labels = ["app", "wired", "compressed", "free"];
    let widths = segment_widths(&parts, inner.width.saturating_sub(3));

    let mut bar = Vec::new();
    for (i, (&w, &color)) in widths.iter().zip(colors.iter()).enumerate() {
        if w > 0 {
            bar.push(Span::styled("█".repeat(w as usize), Style::default().fg(color)));
            if i < widths.len() - 1 {
                bar.push(Span::raw(" "));
            }
        }
    }

    let legend = Line::from(
        labels
            .iter()
            .zip(colors.iter())
            .zip(parts.iter())
            .flat_map(|((l, &c), &p)| {
                vec![
                    Span::styled("■", Style::default().fg(c)),
                    Span::styled(format!(" {l} {} ", fmt_bytes(p)), Style::default().fg(theme::DIM)),
                ]
            })
            .collect::<Vec<_>>(),
    );

    let swap = format!("swap {} / {}", fmt_bytes(mem.swap_used), fmt_bytes(mem.swap_total));

    if font::hero_fits(inner) {
        let used = mem.app + mem.wired + mem.compressed;
        let used_gib = used as f64 / 1_073_741_824.0;
        let hero: Vec<Line> = font::big_text(&format!("{used_gib:.1}G"))
            .into_iter()
            .map(|r| Line::styled(r, Style::default().fg(pcolor)))
            .collect();
        let mut lines = hero;
        lines.push(Line::from(""));
        lines.push(Line::from(bar));
        lines.push(legend);
        lines.push(Line::from(vec![
            Span::styled("pressure ", Style::default().fg(theme::DIM)),
            Span::styled(state, Style::default().fg(pcolor).bold()),
            Span::styled(format!(" · {swap}"), Style::default().fg(theme::DIM)),
        ]));
        f.render_widget(Paragraph::new(lines), inner);
    } else {
        let lines = vec![
            Line::from(vec![
                Span::styled("pressure ", Style::default().fg(theme::DIM)),
                Span::styled(state, Style::default().fg(pcolor).bold()),
            ]),
            Line::from(bar),
            legend,
            Line::styled(swap, Style::default().fg(theme::DIM)),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ui::memory && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/memory.rs
git commit -m "feat: memory card hero with consolidated pressure/swap line"
```

---

### Task 8: Tier switch in layout + integration tests (`src/ui/mod.rs`, `tests/render.rs`)

**Files:**
- Modify: `src/ui/mod.rs:32-49`
- Modify: `tests/render.rs`

**Interfaces:**
- Consumes: every card's self-deciding render (Tasks 3-7).
- Produces: `full = area.height >= 30 && area.width >= 120` drives header `Length(7)` and gauges `Length(12)`; otherwise today's `3`/`10`.

- [ ] **Step 1: Write the failing integration tests**

Add to `tests/render.rs`:

```rust
#[test]
fn full_tier_shows_hero_font_and_housed_fan() {
    let c = draw_at(160, 45);
    assert!(c.contains("╭─────╮"), "housed fan missing");
    assert!(c.contains("█ ▄ █"), "4-row logo W missing");
    assert!(c.contains("▄  █"), "cpu hero '4' glyph missing"); // total_cpu 41 → "41%"
    assert!(c.contains("█  ▄▀"), "hero '%' glyph missing");
}

#[test]
fn compact_tier_keeps_old_visuals() {
    let c = draw_at(80, 24);
    assert!(!c.contains("╭─────╮"), "housed fan must not render at 80x24");
    assert!(!c.contains("█ ▄ █"), "4-row logo must not render at 80x24");
    assert!(c.contains("88.0°C"), "compact temp readout missing");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test render`
Expected: `full_tier_shows_hero_font_and_housed_fan` FAILS (mod.rs still gives header 3 rows and gauges 10 everywhere); `compact_tier_keeps_old_visuals` PASSES already (nothing full-tier renders yet) — that's fine, it pins the behavior.

- [ ] **Step 3: Write the implementation**

In `src/ui/mod.rs`, replace the top of `draw` (the tier flags and `chunks`):

```rust
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let show_ports = area.height >= 20;
    let show_network = area.height >= 16;
    let show_power = area.width >= 70;
    let show_temp = area.width >= 50;
    // Full visual tier: padded header with housed fan, hero-number gauge
    // cards. Needs width for the ~27-col hero strings (4 cards x 30 cols)
    // and height for header 7 + gauges 12 + a useful body.
    let full = area.height >= 30 && area.width >= 120;

    let chunks = Layout::vertical([
        Constraint::Length(if full { 7 } else { 3 }),
        Constraint::Length(if full { 12 } else { 10 }),
        Constraint::Min(6),
    ])
    .split(area);
```

(The rest of `draw` and `render_left_column` stay unchanged; also update the doc comment above `draw` to mention the full/compact tier split.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all unit tests, plus the whole render sweep (`renders_at_all_sizes_without_panic` covers 200×50 … 20×5).

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs tests/render.rs
git commit -m "feat: switch layout to full visual tier at >=120x30"
```

---

### Task 9: Final verification pass

**Files:**
- No new code; fixes only if checks fail.

- [ ] **Step 1: Full test suite**

Run: `cargo test`
Expected: all green.

- [ ] **Step 2: Lints and formatting**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: clean. If clippy flags anything in touched files, fix and re-run.

- [ ] **Step 3: Visual check (human)**

Run: `cargo run` in a large terminal (≥120×30), then resize below 120 cols and below 30 rows.
Verify: padded header with housed spinning fan; four hero cards; per-core strip; smooth fan; compact layouts return when small. This step is a user checkpoint — report done and ask the user to look.

- [ ] **Step 4: Commit any fixups**

```bash
git add -A && git commit -m "chore: post-refresh fixups" # only if anything changed
```
