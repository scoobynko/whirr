# whirr — design spec

**Date:** 2026-07-14
**Status:** approved design, pre-implementation

## What it is

`whirr` is a macOS system-activity dashboard that lives in the terminal. You type
`whirr`, it fills the terminal with a live, visually rich picture of what the
machine is doing, and you leave it running all day. It is named after the sound
of a fan spinning up — and its logo is one.

Target machine: Apple Silicon Macs (developed on an M5, macOS 26). Intel Macs
are out of scope. Published later to crates.io and Homebrew under the name
`whirr` (verified free on both as of 2026-07-14).

## Goals & non-goals

**Goals**

- Glanceable, beautiful, information-dense dashboard: processes, CPU,
  temperature, power, memory pressure, network, localhost ports.
- Cheap enough to run constantly: **< 0.5% average CPU, ~15 MB RAM**, zero
  cost when nothing changes.
- Light interaction: scroll, sort, kill a process. Not an htop replacement.
- Sudo-free. Everything must work as a normal user.
- Single self-contained binary, publishable as a normal Rust crate.

**Non-goals**

- Multiple views/tabs, process drill-down, search, config files, theming.
- Linux/Windows/Intel-Mac support (architecture shouldn't preclude it later,
  but no effort is spent on it now).
- Historical persistence — everything is session-scoped, in-memory.

## Stack

- **Rust** (toolchain installed via rustup), **ratatui** + **crossterm**.
- **sysinfo** crate for processes, CPU %, memory, network counters.
- Direct **IOKit / IOReport / sysctl** bindings for the Apple-specific data
  (see Data sources). Reference implementation: `macmon` (MIT).

## Architecture

Single process. Threads:

| Thread | Cadence | Collects |
|---|---|---|
| fast sampler | 2 s | per-core CPU %, process list, network counters |
| medium sampler | 5 s | temperature, power draw, battery, memory pressure, swap |
| slow sampler | 10 s | listening TCP ports (spawns `lsof`, the one allowed subprocess) |
| UI thread | event-driven | renders on: new snapshot, keypress, resize, animation tick |

Samplers push **immutable snapshot structs** over an `mpsc` channel; the UI
thread owns all state (`App` struct holding latest snapshots + ring buffers of
history for the charts, 60 samples each). No shared mutable state, no locks on
the render path.

The UI thread blocks on the channel/input with a timeout equal to the next
animation frame due — there is no busy polling. When the fan animation is idle
(low CPU load) this is ~2 fps; it scales up to ~10 fps under load. Ratatui
diffs cells, so an animation frame rewrites only the few fan glyphs.

History ring buffers store 60 samples (2 minutes at 2 s cadence) for: total
CPU, temperature, power (per-domain), network up/down.

## Layout

```
┌──────────────────────────────────────────────────────────────────┐
│  ██ whirr ██  (block font)   ✻ fan   macOS 26.3 · Apple M5       │
│                                       up 3d 4h · load 2.31       │
├─ CPU ──────────────┬─ Temp ─────────┬─ Power ───────┬─ Memory ───┤
│ core heatmap grid  │ thermometer    │ hero W number │ pressure   │
│ (P/E cores)        │ gauge + trend  │ + CPU/GPU/ANE │ + segmented│
│ 60s area chart     │ sparkline      │ stacked chart │ bar + swap │
├─ Processes ────────┴────────────────┴─┬─ Network ───┴────────────┤
│ pid name cpu% [▮▮▮ ] mem [▮▮  ] …     │ mirrored waveform        │
│ scrollable, sortable                  │ ▲ up / ▼ down + totals   │
├─ Ports ───────────────────────────────┴──────────────────────────┤
│ :3000 node (my-app) · :5432 postgres · :8080 python3 …           │
└──────────────────────────────────────────────────────────────────┘
```

Layout reflows on resize via ratatui's constraint solver. Below a minimum
usable size (~80×24) panels collapse in priority order: ports → network →
power → temp (processes and CPU survive longest).

### Header

- **Logo**: "whirr" in a FIGlet-style block font, baked in as static string art.
- **Fan**: multi-frame ASCII fan beside the logo. Rotation speed maps to total
  CPU load — barely turning at idle, whirring at full load. `--no-fan` disables
  the animation entirely (renderer then wakes only on data/input).
- Ambient facts fill spare width: chip name, macOS version, uptime, load avg.

### Panels & visualizations

Dataviz principles applied: color encodes one job only (sequential gradient =
magnitude, reserved status colors = state, identity hues = series), borders and
labels stay dim so data is the brightest ink, headline numbers over clutter.
Braille characters (2×4 dots/cell) give high-resolution charts.

- **CPU — chip die heatmap.** Cores drawn as a grid of blocks grouped and
  labeled P-cores / E-cores (laid out like the die, not a list). Each cell
  shades through a single-hue gradient with its load. Below: braille area
  chart of total load, last 60 samples.
- **Temperature — thermometer.** Vertical gauge with bulb, gradient fill,
  braille trend sparkline beside it. Color is status-driven: default hue when
  cool, amber ≥ 85 °C, red ≥ 95 °C. Red always means hot, never decoration.
- **Power — hero number + energy flow.** Total watts in large block font
  (same family as logo), stacked braille area chart of CPU / GPU / ANE watts
  over time. Battery %, charging state, cycle count as a footer line.
- **Memory — pressure, honestly.** Headline is macOS's own pressure state
  (normal / warn / critical, status-colored), then a segmented horizontal bar
  — app / wired / compressed / free with cell gaps — and a swap figure.
- **Network — mirrored waveform.** Download fills up from the center axis,
  upload fills down, braille resolution. Current rates and session totals as
  labels at the ends; peaks marked, nothing labeled per-point.
- **Processes — table that is also a chart.** Columns: pid, name, cpu%, mem.
  CPU% and mem cells contain inline micro-bars. Top consumer subtly
  highlighted. Sorted by CPU by default.
- **Ports — badges.** One badge per listening TCP port: `:port process (pid)`,
  wrapped across the panel width. Deduplicated per port.

### Color

24-bit truecolor with one accent hue; gradients derived from it do all
magnitude work. Status colors (green/amber/red) are reserved for genuine
states (temperature thresholds, memory pressure, kill confirmation). Falls
back to 256-color if the terminal lacks truecolor.

## Interaction

| Key | Action |
|---|---|
| `Tab` | move focus between Processes and Ports panels |
| `↑` / `↓` | scroll focused list |
| `c` / `m` | sort processes by CPU / memory |
| `k` | kill selected process — SIGTERM after a `y/n` inline confirm |
| `q` / `Ctrl-C` | quit |

Focused panel gets a brighter border. No mouse support.

## Data sources (macOS specifics)

| Data | Source | Notes |
|---|---|---|
| processes, CPU %, RAM, network counters | `sysinfo` crate | refresh only the fields needed per tick |
| CPU temperature | SMC sensors via `IOHIDEventSystemClient` | private-but-stable API, sudo-free on Apple Silicon; average of CPU die sensors (macmon technique) |
| power draw (CPU/GPU/ANE watts) | IOReport "Energy Model" channels | sudo-free; delta between samples ÷ interval |
| memory pressure | `sysctl kern.memorystatus_level` | macOS's own signal; map to normal/warn/critical |
| swap | `sysctl vm.swapusage` | |
| battery | IOKit power sources API | %, charging, cycles, health |
| listening ports | `lsof -iTCP -sTCP:LISTEN -P -n` every 10 s | parsed in slow sampler thread; ~100 ms subprocess per 10 s beats walking every process's fds via libproc |
| uptime, load avg, chip/OS name | sysctl / sysinfo | read once (static) or on medium tick |

## Error handling

- **Graceful degradation per sensor**: if any Apple-specific reader fails
  (future macOS change, unexpected chip), its panel renders `n/a` with a dim
  note; the rest of the dashboard is unaffected. No panics from data code.
- **Terminal safety**: panic hook + `Drop` guard restore the terminal
  (raw mode off, alternate screen exited) on any exit path.
- **Kill errors** (e.g. permission denied on system process) surface as a
  transient inline message, not a crash.
- `lsof` failure/timeout → ports panel shows last good data with a stale marker.

## Testing

- **Unit tests** for all parsers and pure logic: lsof output parsing (fixture
  captures), sysctl value interpretation, history ring buffer, load→fan-speed
  mapping, byte/rate formatting.
- **Integration smoke test** (runs on a real Mac): every sampler produces a
  snapshot within its cadence; temperature within 0–120 °C, power 0–200 W,
  pressure in valid range.
- **Render tests** with ratatui's `TestBackend`: dashboard renders without
  panicking at various terminal sizes, including degenerate small ones.
- Manual perf check before calling it done: `whirr` itself observed < 0.5%
  average CPU in its own process table over several minutes.

## Distribution

- Normal publishable crate layout: `cargo install --path .` works; README,
  MIT license, `Cargo.toml` metadata ready for crates.io.
- Local install for daily use: release binary symlinked into
  `/opt/homebrew/bin/whirr`.
- Homebrew formula/tap is future work, deliberately out of scope now.
