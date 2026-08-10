# Releasing

Maintainer notes. Nobody else needs this, which is why it isn't in
[CONTRIBUTING.md](../CONTRIBUTING.md).

## The pipeline

1. PRs land on `main` with conventional commit messages.
2. `release-plz` keeps a `chore: release` PR open, holding the version bump and
   the generated changelog. It rewrites that PR on every push to `main`, so
   several merges batch into one release.
3. **Merging that PR is the only manual step.** It tags the version and
   publishes to crates.io. Deliberately manual: it is the last look before an
   irreversible publish.
4. The tag triggers `cargo-dist`, which builds the `aarch64-apple-darwin`
   binary, creates the GitHub Release with tarball and checksums, and commits
   the updated formula to `scoobynko/homebrew-whirr`.

## On a 0.x crate, `feat:` is a patch bump

cargo semver treats the minor position as the major while the major is 0, so
only a breaking change moves `0.3` to `0.4`. A release with a `feat:` and two
`fix:`es is still a patch. Don't promise a version number without applying
that rule first.

## The token that breaks everything quietly

`release-plz` authenticates with `RELEASE_PLZ_TOKEN`, a PAT, **not** the
default `GITHUB_TOKEN`. GitHub refuses to trigger workflows from events created
with the default token, so if release-plz tagged with it, step 4 above would
never fire.

Nothing errors. You get a real tag and a real crates.io version, no binary, and
a tap formula still pointing at the previous release. It surfaces when somebody
runs `brew install` and gets the old version.

If a release ever produces a tag but no GitHub Release, check that token first.

## Secrets

| Secret | Used by | Scope |
|---|---|---|
| `RELEASE_PLZ_TOKEN` | `release-plz.yml` | PAT: Contents + Pull requests, read/write on `whirr` |
| `HOMEBREW_TAP_TOKEN` | `release.yml` | PAT: Contents read/write on `homebrew-whirr` |
| `CARGO_REGISTRY_TOKEN` | `release-plz.yml` | crates.io token, scoped to the `whirr` crate |

One PAT scoped to both repositories, stored under both of the first two names,
is fine. One credential to rotate.

## Things that have gone wrong

- **The release job on `ubuntu-latest`.** `cargo publish` verifies by building,
  and whirr cannot compile on Linux (`#[link(kind = "framework")]`, `E0455`).
  It runs on `macos-14`.
- **The tap repo was empty.** No commits, no default branch, so cargo-dist had
  nothing to commit onto. That failure lands *after* the crates.io publish,
  which cannot be undone.
- **`dist init` writes `lto = "thin"`** into `[profile.dist]`, quietly
  overriding the fat LTO in `[profile.release]`. Remove it every time. It is
  the difference between 853K and 1.0M.
- **Both `main` branches are protected by rulesets, not classic branch
  protection.** Checking `branches/main/protection` returns 404 and looks
  unprotected. Every merge needs `gh pr merge N --admin`.
