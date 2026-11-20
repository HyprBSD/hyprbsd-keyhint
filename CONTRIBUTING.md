# Contributing to hyprbsd-keyhint

Bug reports, fixes and new features are welcome. Open an issue or a pull request on GitHub,
against `main`. For a security problem, don't open an issue: see [SECURITY.md](SECURITY.md).

A bug report helps most with the FreeBSD version (`freebsd-version -ku`), hyprbsd-keyhint's
version (on HyprBSD, `pkg info hyprbsd-keyhint`; built from source, the tag or commit), what you
did, where the card showed or didn't, and `hyprctl -j clients` for the program's window.

## Before a pull request

- `make check` and `make deny` pass. GitHub runs rustfmt's check, cargo-deny and `make check`
  in FreeBSD on every push and pull request (`.github/workflows/check.yml`).
- A change in behaviour comes with a test that fails without it.
- Code is formatted by `cargo fmt` (`rustfmt.toml`) and clean under clippy with warnings denied.
  Comments say what the code does, briefly.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/):
  `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:` and the rest. The changelog is made
  from them with git-cliff (`cliff.toml`), which leaves out any other commit.

Contributions are under the BSD-2-Clause licence, as the rest of hyprbsd-keyhint (`LICENSE`).

## Tests

`make check` runs the unit tests, then `tests/session-test.sh` on a release build: the real
program in a headless sway with two outputs and a bar, its own D-Bus session, a stand-in for
Hyprland's sockets (`tests/fake-hyprland.py`) and one for wofi. It checks how desktop entries
start (arguments, terminal, actions, D-Bus activation, `DRI_PRIME`), where the card goes and
when it closes, which window is the program's and which entry a window is from, and the keys
dialog, with `wtype` pressing the keys. It needs `sway`, `grim`, `jq`, `wtype`, `python312` with
`py312-pygobject`, and `dbus`, and touches nothing outside its temporary directory.

What it can't show is Hyprland itself: run a changed build in a real Hyprland session too.

## Releases

A release is a tag, `v` and the version in `Cargo.toml`, for example `v0.1.0`, with its source
tarball attached. It needs `git-cliff` (FreeBSD's package):

1. Set the version in `Cargo.toml`, and run `make check` and `make deny`.
2. Add the release to the changelog, commit it with the new version, then tag it:

   ```
   git cliff --unreleased --tag v<version> --prepend CHANGELOG.md
   git commit -m "chore(release): prepare for v<version>" Cargo.toml Cargo.lock CHANGELOG.md
   git tag -a v<version> -m "hyprbsd-keyhint <version>"
   ```

   Then `git status` shows nothing to commit, and `git show v<version>:Cargo.toml` has the new
   version.
3. Push `main` and the tag, and wait for GitHub's checks to pass.
4. `make dist` makes `target/hyprbsd-keyhint-<version>.tar.gz` from the tag. Make the tag's
   GitHub release with that tarball attached and `git cliff --latest --strip header` as its notes.

The port fetches that tarball, so it must not change once the release is out: a fix is a new
version.
