# whirr v0.2 tweaks — design spec

**Date:** 2026-07-15
**Status:** approved design, pre-implementation
**Baseline:** whirr v0.1 on `main` (34 tests green)

## What changes

Two user-requested tweaks:

1. **Process table shows exactly the top 10** — no scrolling into a longer list.
2. **Ports card shows the project behind each port** — `:3000 node (my-app)`
   instead of `:3000 node`, so a dev server is identifiable at a glance.

Everything else (panels, keys, samplers, perf budget) is unchanged.

## 1. Top-10 process table

- The fast sampler continues to send its candidate set (union of top-50 by
  CPU and top-50 by memory) — this population is required so the `m` sort
  can still surface memory whales.
- The UI displays at most **10 rows** after the active sort. `↑`/`↓` select
  within those 10 only; selection clamps to index 9. Kill flow unchanged.
- Layout: the middle row (processes + network) becomes a fixed
  `Length(13)` (10 rows + 1 footer + 2 borders); the ports row changes from
  `Length(4)` to `Min(4)`, absorbing the freed vertical space.
- Collapse behavior at small terminal sizes keeps the same priority order
  (ports → network → power → temp); the render-test suite is updated for
  the new geometry.

## 2. Ports card with project info

- `PortInfo` gains `project: Option<String>` — the basename of the owning
  process's current working directory.
- Source: new helper `mac::proc::cwd_basename(pid: i32) -> Option<String>`
  using `proc_pidinfo(PROC_PIDVNODEPATHINFO)` (libproc, sudo-free for the
  user's own processes — which dev servers are). Returns `None` for other
  users' processes or any FFI failure; never panics.
- Enrichment happens in the **slow sampler** after `parse_lsof`, once per
  10s tick, one syscall per unique pid (typically < 20 — microseconds).
  A per-tick `HashMap<pid, Option<String>>` avoids duplicate syscalls when
  one process listens on several ports. No extra subprocess.
- Rendering: badge becomes `:3000 node (my-app)` — project in dim
  parentheses after the process name; omitted entirely when `None`
  (system daemons look exactly as they do today).
- The stale-resend path keeps the last good enriched entries (project
  info rides along in `last_good`).

## Non-goals

- Command-line args (`npm run dev`) — considered, not selected.
- Bind-address or PID display in the badge.
- Scrolling the process table beyond 10 (explicitly removed).

## Testing

- `parse_lsof` tests updated: parser output has `project: None` (parser
  never fills it; enrichment is a separate step) — plus a small unit test
  for the enrichment merge.
- `cwd_basename` integration test: calling it on our own pid returns the
  basename of `std::env::current_dir()`.
- Render tests updated for the new layout geometry (processes panel fixed
  height, ports `Min(4)`); tiny-size collapse expectations re-checked.
- Selection clamp test: with 30 processes ingested, `↓` pressed 15 times
  → selected == 9.

## Error handling

- `proc_pidinfo` failure → `None` → badge renders without parens.
- All new FFI follows the established `mac/` conventions: `Option`
  returns, no panics, tightly scoped `unsafe`.

## Perf

- Slow tick only; +1 syscall per listening pid per 10s. No measurable
  impact on the < 0.5% CPU budget.

## Layout revision (2026-07-15, post-v0.2 feedback)

The ports card moves from a full-width bottom row into the **left column,
under the processes table** (user-selected mockup):

```
┌─ header ──────────────────────────┐
├─ CPU ─┬─ Temp ─┬─ Power ─┬─ Mem ──┤
├─ Processes (10) ──┬─ Network ─────┤
│ …                 │ ▼▲ waveform   │
├─ Ports ───────────┤ (full body    │
│ :3000 node (app)  │  height)      │
└───────────────────┴───────────────┘
```

- Body splits horizontally first: left `Ratio(3,5)`, right `Ratio(2,5)`.
- Left column stacks processes (`Max(MAX_VISIBLE_PROCS + 3)`) over ports
  (`Min(4)`); the same solver-verified graceful collapse applies within
  the column at heights 20–29.
- Network spans the full body height in the right column.
- Collapse priority and thresholds unchanged: ports drop below height 20
  (left column = processes only), network drops below height 16 (left
  column takes full width).
- Render-test geometry assertions updated for the new arrangement.
