# whirr

A macOS system dashboard that lives in your terminal. Run `whirr`, leave it
running all day: processes, CPU, temperature, power draw, memory pressure,
network throughput, localhost dev servers and Claude Code sessions — all on one
glanceable screen. Named after the sound of a fan spinning up (its logo is one).

```
https://github.com/user-attachments/assets/0cd0abed-fcae-4c5e-893d-05d02d69a0e7

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
Macs are out of scope — the IOReport power channels whirr reads don't exist
there.

## Install

```bash
brew install scoobynko/whirr/whirr
```

Or, without Homebrew:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/scoobynko/whirr/releases/latest/download/whirr-installer.sh | sh
```

Or from crates.io, if you have a Rust toolchain:

```bash
cargo install whirr
```

Both the tap and the installer script ship a prebuilt `aarch64-apple-darwin`
binary — no Rust toolchain required.

<details>
<summary>From source</summary>

```bash
git clone https://github.com/scoobynko/whirr && cd whirr
cargo build --release
ln -sf "$(pwd)/target/release/whirr" /opt/homebrew/bin/whirr
```

</details>

## The screen

whirr picks one of three layouts from the terminal size. All three share the
same design — the burst fan, the bitmap wordmark, the same cards — and differ
only in how much room each card gets:

| Size | Layout |
|---|---|
| ≥ 120×30 | Four hero-number gauge cards in one row; processes, network, and three separate cards for localhost / Claude sessions / other listeners |
| ≥ 70×40 (narrower) | The same hero cards, stacked 2×2 across two bands — a tall, narrow window keeps the full design rather than dropping to compact |
| anything smaller | Plain readouts instead of hero numbers, and one combined Ports card |

Panels drop out in priority order as space gets tight: ports first, then
network, then power, then temperature. Processes and CPU always survive.

## Keys

| Key | Action |
|---|---|
| `Tab` | cycle focus: processes → localhost → Claude sessions → others |
| `↑` / `↓` | move the selection within the focused card |
| `c` / `m` | sort processes by CPU / memory |
| `k` | kill the selected process or dev server (SIGTERM, `y`/`n` to confirm) |
| `q` / `Ctrl-C` | quit |

`k` is deliberately inert on the Claude sessions and others cards: ending a
session mid-conversation, or killing a system agent, shouldn't be one keypress
away.

## Flags

| Flag | Effect |
|---|---|
| `-h`, `--help` | print usage and exit |
| `-V`, `--version` | print the version and exit |
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

Note: the budget is measured on an interactive terminal at normal QoS;
sandboxed/background-QoS runs report inflated CPU (~5×) because the whole
process is scheduled on efficiency cores at idle clocks.

## Releases

Releases are automated from [conventional commits](https://www.conventionalcommits.org):
`release-plz` opens a release PR with the version bump and changelog, and
merging it tags the version, publishes to crates.io, and triggers `cargo-dist`
to build the binary and update the tap. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT, see [LICENSE](LICENSE).
