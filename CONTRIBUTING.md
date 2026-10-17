# Contributing

Mr. Usage is one maintainer's project, and changes from anyone else are welcome through the same door: a pull request against `main`. This page says what that door looks like, so you can get a change in without a round of "please also".

## Report a bug or ask for something

Open an issue on GitHub. Say which provider card is involved, what the card showed, and what you expected. If the number is wrong, paste the vendor's own usage page or API response with the key removed, since most card bugs turn out to be a field the vendor renamed.

Security problems go through [SECURITY.md](SECURITY.md), not the public issue tracker.

## How a change gets in

1. Fork the repository and make the change on a branch.
2. Open a pull request against `main`. Write what changed and why in the description, the same way the commit messages in `git log` do.
3. CI runs the checks below on Linux, macOS and Windows. `main` is protected by a repository ruleset: it takes pull requests only, every required check must pass, and force pushes are blocked. There is no way around a red check, including for the maintainer.
4. Once the checks are green the pull request is merged. Small, single-purpose pull requests merge fastest.

## What a change has to satisfy

Every pull request is held to the same rules the existing code follows.

- `cargo fmt --all -- --check` passes. The only formatting setting is `max_width = 140` in `rustfmt.toml`.
- `cargo clippy --workspace --all-targets -- -D warnings` passes. The workspace denies `unsafe_code`, and denies `unwrap`, `expect`, `panic!`, `dbg!` and the print macros outside tests (`clippy.toml` allows them in tests).
- `cargo test --workspace` passes on all three operating systems.
- `cargo deny check` and `cargo audit -D warnings` stay clean. A new advisory is ignored only with a written reason in `deny.toml`, `.cargo/audit.toml` and `osv-scanner.toml`, and an entry in `docs/FOLLOWUPS.md` saying when the ignore can go.
- GitHub Actions are pinned to a full commit SHA with the version in a trailing comment, as every workflow in `.github/workflows/` already does.
- New functionality comes with tests. A new provider gets its documented example responses as fixtures and a test per documented error status. A bug fix gets a regression test that fails without the fix. Tests are not optional extras to add later.
- Every fact about a vendor (endpoint, auth header, response fields, whether the call is free) is recorded in `docs/SOURCES.md` with the page it came from and the date it was read. A guess is marked `TODO(verify)` in the code.
- The core crate never deserializes anything that identifies the account: no emails, names, key labels or ids. `crates/core/tests/source_audit.rs` checks for this and will tell you where to register a new struct.
- If a user of the app would notice the change, add one line under "Unreleased" in [CHANGELOG.md](CHANGELOG.md). Internal fixes, CI and documentation changes stay out of it. The release workflow turns that section into the release notes.

The minimum supported Rust version is 1.90, checked in CI on the core crate. The frontend builds with Node 22.

## Running the checks yourself

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check && cargo audit -D warnings
npm run typecheck && npm audit --audit-level=low
```

[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) covers the build, the Linux-only keystore and end-to-end tests, the installer smoke tests, fuzzing with `cargo +nightly fuzz`, and the step-by-step guide for adding a provider.

## Adding a provider

Follow "Adding a provider" in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). The short version: the vendor must document a usage endpoint that works with an ordinary API key, the app contacts exactly that host and nothing else, and the card shows only numbers and dates. If the vendor has no such endpoint, the answer is a line in the "Providers considered and not added" list in `docs/FOLLOWUPS.md`, not a scraper.

## Licence

The project is dual-licensed under MIT or Apache-2.0, at the user's choice. By submitting a change you agree that it is licensed the same way, unless you say otherwise in the pull request.
