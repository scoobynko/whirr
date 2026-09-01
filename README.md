<img src="assets/whirr-lockup.gif" width="380" alt="whirr">

macOS system dashboard for your terminal. No sudo, no mouse.

[Watch it run](https://github.com/user-attachments/assets/c3844884-5440-4cba-9b32-f073f100a815)

## Why

We spent years in fancy web UIs. Now we are back in the terminal, hours a day.

So I wanted this in the same window. Not another app to alt-tab to.

Then I added what I keep losing track of. Localhost servers, because I run a
few projects at once and always forget one. Claude sessions, because I run
several agents in parallel and I want to know which one is cooking the machine.
And which one is still going while I am not looking. My opinion on Claude Code
is well known.

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
| `d` | what a Claude session is doing. in full |
| `o` | open a dev server in your browser, or jump to the terminal a Claude session runs in |
| `k` | kill it. asks first |
| `s` | settings: theme, accent, background, fan |
| `q` | quit |

Settings stick in `~/.config/whirr/config.toml`.

## Claude sessions

Every session that is running, whether or not it holds a port. With what it is
doing next to it.

Filled circle is working. Half filled is running with nobody watching. Hollow
is waiting for you. The word says which. `busy ×2` counts subagents, `loop 4m`
counts down to a wakeup the session set for itself, `bg job` is a shell that
outlived its turn, `scheduled` is one a cron or a `/schedule` wakes up,
`idle 14d` is one you forgot about.

Amber means it is running without you. That is the whole point of the card.

None of this is guessed from CPU. Claude Code writes its own busy flag and
whirr reads it. A session that says it is busy but stopped writing goes amber
too, because that one is stuck, not working.

Second account included. whirr reads every `~/.claude*` config root, not just
the default one.

`d` opens the rest. Which subagent, on which task, the real command a
background shell is running, the full path, the account, how long it has been
open.

No countdown on `scheduled`, though. The schedule itself lives on the server,
so whirr can tell you it happens. Not when.

## Performance

Under 0.5% CPU, under 25 MB, left running all day.

The process table is read with raw `libproc` calls, about 1 ms a pass instead
of the 35 ms a full `sysinfo` refresh costs. Three sampler threads at 2, 5 and
10 seconds. The screen redraws only when something changed.

Working out what every Claude session is doing costs about 4 ms, on the 10
second tick. Transcripts are read 64 KB from the tail, and only for the
sessions sitting still. The one writing to its own log never pays for it.

## One network request

Once a day whirr asks crates.io if there is a newer version, and says so in the
footer if there is. Nothing else leaves your machine. `--no-update-check` and
there are no requests at all.

## License

MIT. See [LICENSE](LICENSE).
