# Contributing

## Commits

Commit messages follow [conventional commits](https://www.conventionalcommits.org).
This is not a style preference — it is the input to the release.

### How the version number is decided

**From the commit messages, not from branch names.** Branch names are never
consulted by anything in the pipeline.

When `release-plz` runs, it collects every commit since the last git tag, reads
each one's prefix, takes the **largest** bump any single commit asks for, and
applies it to the current version in `Cargo.toml`:

| Prefix | Bump | Changelog section |
|---|---|---|
| `feat:` | **minor** — `0.3.0` → `0.4.0` | Added |
| `fix:` | patch — `0.3.0` → `0.3.1` | Fixed |
| `perf:` | patch | Performance |
| `refactor:` | patch | Changed |
| `docs:` | patch | Documentation |
| `test:` `chore:` `ci:` `build:` `style:` `revert:` | none | *(hidden)* |
| any of the above with `!`, e.g. `feat!:` | **major** — `0.3.0` → `1.0.0` | Added, flagged breaking |
| a `BREAKING CHANGE:` footer in the body | **major** | as above |

So a batch of ten `fix:` commits is still one patch bump, and a single `feat:`
among them makes it a minor. A release containing only `chore:`/`ci:` commits
proposes no release at all.

The full grammar is `type(optional-scope)!: subject`, e.g.
`fix(ports): keep the selection visible`. Scope is free-form and affects
nothing but readability.

### Why this is enforced rather than suggested

A message release-plz cannot parse is **silently ignored**: it contributes
nothing to the version bump and never appears in the changelog. Nothing errors.
`Fixed the ports card` looks like a fix and counts for nothing.

Two guards, deliberately unequal:

- **`scripts/commit-msg-lint.sh`**, run by the `commits` job on every PR. This
  is the gate — it cannot be skipped.
- **`.githooks/commit-msg`**, the same script as a local hook, so you find out
  before pushing rather than after. Opt in once per clone:

  ```bash
  git config core.hooksPath .githooks
  ```

  Advisory only: `git commit --no-verify` skips it, and it only exists for
  whoever enabled it. That is why CI runs the check too.

To see what a message would do before committing:

```bash
./scripts/commit-msg-lint.sh -m "feat: your subject here"
```

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
