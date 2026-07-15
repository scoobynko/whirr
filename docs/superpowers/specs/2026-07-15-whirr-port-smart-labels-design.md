# whirr — smart port labels design spec

**Date:** 2026-07-15
**Status:** approved design
**Baseline:** whirr v0.2 on `main`

## Problem

The ports card shows the lsof command name, which for version-named
binaries is useless: Claude Code sessions appear as `2.1.187 (ownit-vibe)`
because the executable is `~/.local/share/claude/versions/2.1.187`.

## What changes

Rows for the user's own processes get a **smart label**: real app name +
a short args hint.

```
:61379  claude --session-id (ownit-vibe)   ← was  2.1.210 (ownit-vibe)
:63465  claude --bg-spare (ownit-vibe)     ← was  2.1.187 (ownit-vibe)
:3001   node server.js (my-app)            ← was  node (my-app)
```

System daemons (args unreadable without root) keep their lsof name.

## Data source

`mac::proc::args(pid) -> Option<ProcArgs>` reading the `KERN_PROCARGS2`
sysctl (buffer: `[argc:i32][exec_path\0][NUL padding][argv0\0 argv1\0 …]`).

```rust
pub struct ProcArgs {
    pub exec_path: String,  // true binary path (more reliable than argv[0])
    pub argv: Vec<String>,  // argv[0..argc]
}
```

Sudo-free for own processes; `None` on any failure. Called once per unique
pid per 10s slow tick (cached like the cwd lookup). Never panics.

## Label heuristic (pure function, table-tested)

`smart_label(raw: &str, args: Option<&ProcArgs>) -> String`

1. **Name part** from `exec_path` basename:
   - version-like (`2.1.187`, `v18.2.0` — `v?` + ≥2 dot-separated numeric
     parts) → walk ancestors past generic components (`versions`, `bin`,
     `sbin`, `libexec`, `contents`, `macos`, `current`, version-like,
     single chars) to the first meaningful one (`claude`); fallback `raw`.
   - known interpreter (`node`, `python*`, `bun`, `deno`, `ruby`, `java`,
     `perl`, `sh`, `bash`, `zsh`) whose `argv[0]` basename differs → use
     the argv[0] basename (`npm`), else keep the interpreter name.
   - otherwise → the basename itself.
2. **Hint part** from `argv[1..]`:
   - first token starting with `-` → that flag, value stripped at `=`;
   - else first non-flag token's basename, plus the following token if
     both are short (`server.js`, `run dev`);
   - none → no hint.
3. `"{name} {hint}"`, truncated to 28 chars (char-safe).
4. `args == None` → return `raw` unchanged.

## Plumbing

Parser unchanged (raw lsof name). The slow sampler's enrichment pass gains
`apply_smart_labels(&mut ports, mac::proc::args)` — per-unique-pid cache,
rewrites `PortInfo.process` in place. UI unchanged. Any failure leaves the
row exactly as today.

## Testing

- Table-driven `smart_label` tests: claude/version case, `node server.js`,
  `npm run dev` (interpreter + differing argv0), flag with `=value`,
  no-args fallback, truncation.
- `apply_smart_labels` cache test (one lookup per unique pid).
- Integration: `args(own_pid)` returns our own argv and a plausible
  exec_path.
- Existing parser/render tests unchanged.

## Perf

Slow tick only; one extra sysctl per unique listening pid per 10s.
