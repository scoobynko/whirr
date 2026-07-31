# Contributing

## Commits

Commit messages follow [conventional commits](https://www.conventionalcommits.org).
This is not a style preference — `release-plz` reads them to decide the next
version number and to write `CHANGELOG.md`, so the prefix you choose has a
direct effect on what ships:

| Prefix | Version bump | Appears in the changelog as |
|---|---|---|
| `feat:` | minor | Added |
| `fix:` | patch | Fixed |
| `perf:` | patch | Performance |
| `refactor:` | patch | Changed |
| `docs:` | patch | Documentation |
| `test:`, `chore:`, `ci:`, `build:` | none | *(hidden)* |

A `!` after the prefix (`feat!:`) or a `BREAKING CHANGE:` trailer forces a
major bump.

## Checks

CI runs on macOS arm64, because several tests read this machine's real sensors
rather than fixtures. Locally:

```bash
cargo clippy --all-targets -- -D warnings   # the enforced gate
cargo test
```

`cargo fmt --check` deliberately fails repo-wide: the codebase is hand-formatted
for density, and rustfmt would undo that. Match the surrounding style instead.

## How a release happens

1. Merge PRs to `main` with conventional commit messages.
2. `release-plz` keeps a `chore: release` PR open, holding the version bump and
   the generated changelog. It updates itself on every push to `main`.
3. **Merging that PR is the only manual step.** It tags the version and
   publishes to crates.io.
4. The tag triggers `cargo-dist`, which builds the `aarch64-apple-darwin`
   binary, creates the GitHub Release with tarball and checksums, and commits
   the updated formula to `scoobynko/homebrew-whirr`.

### Why the token matters

`release-plz` authenticates with `RELEASE_PLZ_TOKEN`, a PAT — **not** the
default `GITHUB_TOKEN`. GitHub refuses to trigger workflows from events created
with the default token, so if release-plz tagged with it, step 4 would never
fire. Nothing would error: you would get a real tag and a real crates.io
version, no binary, and a tap formula still pointing at the previous release.
It only surfaces when someone runs `brew install`.

If a release ever produces a tag but no GitHub Release, check that token first.

### Secrets

| Secret | Used by | Scope |
|---|---|---|
| `RELEASE_PLZ_TOKEN` | `release-plz.yml` | PAT: Contents + Pull requests, read/write on `whirr` |
| `HOMEBREW_TAP_TOKEN` | `release.yml` | PAT: Contents read/write on `homebrew-whirr` |
| `CARGO_REGISTRY_TOKEN` | `release-plz.yml` | crates.io token, scoped to the `whirr` crate |

One PAT scoped to both repositories, stored under both of the first two names,
is fine and means one credential to rotate.

## Platform

The build targets `aarch64-apple-darwin` only. This is forced by the code:
`src/mac/ioreport.rs` expects the IOReport "Energy Model" group, which does not
exist on Intel Macs, so an Intel build would compile and then panic on launch.
The generated Homebrew formula gates on arm64 so `brew` refuses cleanly.
