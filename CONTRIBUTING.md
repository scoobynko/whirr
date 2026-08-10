# Contributing

Open a PR. I merge. That is the whole process.

## Before you push

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

Both have to pass. CI runs them on macOS arm64, because some tests read this
machine's real sensors instead of fixtures.

Don't run `cargo fmt`. The code is hand-formatted for density and rustfmt
undoes that. Match whatever is around you.

## Commit messages

[Conventional commits](https://www.conventionalcommits.org). Not a style
preference. The release reads them to decide the version and write the
changelog.

| Prefix | What it does |
|---|---|
| `feat:` | minor bump |
| `fix:` `perf:` `refactor:` `docs:` | patch bump |
| `test:` `chore:` `ci:` `build:` `style:` `revert:` | nothing, hidden from the changelog |
| any of them with `!` | major bump |

A message that doesn't parse is dropped from both the version and the
changelog, silently. Nothing errors. `Fixed the ports card` looks like a fix
and counts for nothing.

CI checks it. To find out before pushing:

```bash
./scripts/commit-msg-lint.sh -m "fix(ports): keep the selection visible"
```

Or turn on the local hook once per clone:

```bash
git config core.hooksPath .githooks
```

## Platform

Apple Silicon only, and that is forced by the code. `src/mac/ioreport.rs`
expects the IOReport "Energy Model" group, which Intel Macs don't have, so an
Intel build would compile and then panic on launch.

Some sensor tests skip on CI, which runs on a virtualised runner with no real
power channels. Run them locally before touching anything in `src/mac`.

---

Releasing is [documented separately](docs/RELEASING.md). You don't need it.
