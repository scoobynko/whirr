# whirr

A macOS system dashboard that lives in your terminal. Run `whirr`, leave it
running all day: processes, per-core CPU heatmap, temperature, power draw,
memory pressure, a network waveform, and localhost ports — all in one glanceable
screen. Named after the sound of a fan spinning up (its logo is one).

```
(screenshot placeholder — paste a terminal capture of `whirr` here)
```

## Why

Everything it shows normally requires either Activity Monitor (mouse-driven,
one view at a time) or `sudo powermetrics` (root, and it owns your terminal).
`whirr` reads the same IOKit / IOReport / sysctl data `powermetrics` does, but
through the read-only channels a normal user account already has access to —
no `sudo`, ever. The one trade-off: processes owned by other users (e.g.
`WindowServer`) don't expose CPU/memory counters without root, so they are not
listed.

Built for Apple Silicon. Developed and tested on an M5 running macOS 26; Intel
Macs are out of scope.

## Install

```bash
cargo install --path .
```

Or build a release binary and symlink it onto your `PATH`:

```bash
cargo build --release
ln -sf "$(pwd)/target/release/whirr" /opt/homebrew/bin/whirr
whirr
```

## Keys

| Key | Action |
|---|---|
| `Tab` | switch focus between Processes and Ports |
| `↑` / `↓` | scroll the focused list |
| `c` / `m` | sort processes by CPU / memory |
| `k` | kill the selected process (SIGTERM, `y`/`n` to confirm) |
| `q` / `Ctrl-C` | quit |

## Flags

| Flag | Effect |
|---|---|
| `--no-fan` | disable the fan animation; the UI then only redraws on new data, a keypress, or a resize — useful on battery or over a slow SSH link |
| `--list-sensors` | print the raw HID temperature sensors and IOReport Energy Model channels this Mac exposes, then exit (no TTY needed) |

## Performance budget

`whirr` is meant to be left running indefinitely, so it's held to a budget:
**< 0.5% average CPU** and **< 25 MB RSS**. Three sampler threads (2 s / 5 s /
10 s cadence) push immutable snapshots over a channel; the UI thread sleeps
until input, data, or the next animation frame, and only redraws when
something actually changed. The process table is scanned with raw `libproc`
calls (one `proc_pidinfo` per pid, names cached) — ~1 ms per pass instead of
the ~35 ms a full `sysinfo` process refresh costs.

## License

MIT, see [LICENSE](LICENSE).
