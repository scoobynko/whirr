# whirr

A macOS system dashboard that lives in your terminal. Run `whirr`, leave it
running all day: processes, CPU, temperature, power draw, memory pressure,
network throughput, localhost dev servers and Claude Code sessions — all on one
glanceable screen. Named after the sound of a fan spinning up (its logo is one).

https://github.com/user-attachments/assets/0cd0abed-fcae-4c5e-893d-05d02d69a0e7

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
| `o` | open the selected dev server in your browser (localhost card only) |
| `k` | kill the selected process or dev server (SIGTERM; a dialog asks first — `y` confirms, `n` or `Esc` cancels) |
| `s` | open settings: theme, accent colour, background, fan |
| `q` / `Ctrl-C` | quit |

`k` is deliberately inert on the Claude sessions and others cards: ending a
session mid-conversation, or killing a system agent, shouldn't be one keypress
away. `o` lives only on the localhost card — a Claude session has no URL, and
a system daemon's port is not something to point a browser at.

A process often listens on more than one port, so `o` opens straight away only
when there's one sensible candidate; otherwise it asks which. Ports the OS
allocated itself (49152 and above) are never offered — they're not pages
anyone means to visit — which is usually enough to leave a single candidate
and no question. Nothing in a port *number* says which one you meant, so
whirr asks rather than guessing.

## Flags

| Flag | Effect |
|---|---|
| `-h`, `--help` | print usage and exit |
| `-V`, `--version` | print the version and exit |
| `--no-fan` | disable the fan animation; the UI then only redraws on new data, a keypress, or a resize — useful on battery or over a slow SSH link |
| `--list-sensors` | print the raw HID temperature sensors and IOReport Energy Model channels this Mac exposes, then exit (no TTY needed) |
| `--no-update-check` | don't ask crates.io whether a newer release exists |

## The one network request

Everything on the dashboard is read locally. The single exception is a version
check: once a day at most, whirr asks crates.io for the latest published
version and, if it's newer than the one running, says so in the footer along
with the command that upgrades *your* installation — Homebrew, cargo, or the
installer script, inferred from where the binary lives.

It sends nothing but a `User-Agent` identifying whirr and its version (which
crates.io requires), it runs on its own thread so it can never delay startup
or a redraw, and the answer is cached in
`~/.cache/whirr/update-check`. Turn it off with `--no-update-check`, and
whirr makes no network requests at all.

## Settings

`s` opens a dialog for the things that are a matter of taste: a light or dark
palette, one of five accent colours, whether the fan animates, and whether
whirr paints the frame background at all.

That last one matters if your terminal is themed or translucent — whirr paints
edge to edge by default, which overrides it. Set **background** to `terminal`
and the frame is left alone.

Changes apply as you make them and are remembered in
`~/.config/whirr/config.toml` (or `$XDG_CONFIG_HOME/whirr/`):

```toml
[appearance]
theme = "dark"        # dark | light
accent = "teal"       # teal | blue | violet | amber | green
background = "painted" # painted | terminal

[behaviour]
fan = true
```

The file is optional and edits to it are picked up on the next launch. A value
whirr doesn't recognise is ignored and that one setting falls back to its
default — a typo in the accent won't discard your theme — and an unparseable
file is ignored entirely rather than being an error worth refusing to start
over. Flags win over the file, so `--no-fan` applies for that run without
rewriting your preference.

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
