<img src="assets/whirr-lockup.gif" width="380" alt="whirr">

macOS system dashboard for your terminal. No sudo, no mouse.

[Watch it run](https://github.com/user-attachments/assets/c3844884-5440-4cba-9b32-f073f100a815)

## Why

We spent years in fancy web UIs. Now we are back in the terminal, hours a day.

So I wanted this in the same window. Not another app to alt-tab to.

Then I added what I keep losing track of. Localhost servers, because I run a
few projects at once and always forget one. Claude sessions, because I run
several agents in parallel and I want to know which one is cooking the machine.
My opinion on Claude Code is well known.

## Install

```bash
brew install scoobynko/whirr/whirr
```

No Homebrew, or Homebrew is shouting at you about Xcode:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/scoobynko/whirr/releases/latest/download/whirr-installer.sh | sh
```

Rust toolchain:

```bash
cargo install whirr
```

Then run `whirr`.

Apple Silicon only. Intel Macs don't expose the power channels this reads, so
there would be nothing to show.

## Keys

| | |
|---|---|
| `tab` | move between cards |
| `↑` `↓` | move inside one |
| `c` `m` | sort processes by cpu, by memory |
| `o` | open a dev server in your browser, or jump to the terminal a Claude session runs in |
| `k` | kill it. asks first |
| `s` | settings: theme, accent, background, fan |
| `q` | quit |

Settings stick in `~/.config/whirr/config.toml`.

## Performance

Under 0.5% CPU, under 25 MB, left running all day.

The process table is read with raw `libproc` calls, about 1 ms a pass instead
of the 35 ms a full `sysinfo` refresh costs. Three sampler threads at 2, 5 and
10 seconds. The screen redraws only when something changed.

## One network request

Once a day whirr asks crates.io if there is a newer version, and says so in the
footer if there is. Nothing else leaves your machine. `--no-update-check` and
there are no requests at all.

## License

MIT. See [LICENSE](LICENSE).
