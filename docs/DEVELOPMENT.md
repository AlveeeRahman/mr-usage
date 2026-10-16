# Development

For people changing the code. Users only need the [README](../README.md).

## Build

You need Rust stable (1.90 or newer), Node.js 20.19 or newer, and [Tauri's system packages](https://v2.tauri.app/start/prerequisites/) for your OS. On Debian or Ubuntu: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libxdo-dev libssl-dev`.

```sh
git clone https://github.com/AlveeeRahman/mr-usage.git
cd mr-usage
cargo install tauri-cli --version 2.12.3 --locked   # once
cargo tauri dev                                     # installs npm packages and starts the Vite dev server for you
cargo tauri build                                   # installers land in target/release/bundle/
```

`npm run tauri dev` does the same with the CLI pinned in `package-lock.json`.

## Layout

```
crates/core/       providers, keystore, redaction, HTTP, poller, midnight wipe, settings. Plain cargo, no Tauri.
src-tauri/         the Tauri shell: commands, capabilities, CSP, window, signal handling, --forget-keys
src/               the window (TypeScript, no framework)
install.sh         user-facing installer for macOS and Linux (also the uninstaller)
install.ps1        the same for Windows
docs/              threat model, sources, review, follow-ups, the GitHub Pages site
.github/           workflows, the installer smoke tests, the end-to-end test, the leak test, build helpers
```

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings   # no unwrap, expect, panic! or print macros outside tests
cargo test --workspace                                   # unit tests, redaction, settings, allocator leak scan, source audit
cargo deny check && cargo audit -D warnings
npm run typecheck && npm audit --audit-level=low
```

Linux only, with an unlocked Secret Service running:

```sh
cargo test -p mr-usage-core -- --ignored --test-threads=1   # real keystore round trip and leak scan
bash .github/scripts/leak-test-linux.sh                     # core-dump leak test (needs gdb)
bash .github/e2e/run-linux.sh                               # drives the real app through WebDriver
```

Installer tests, which build the real packages and then install, start, stop (three ways) and uninstall them:

```sh
bash .github/scripts/smoke-linux.sh deb target/release/bundle/deb/*.deb
bash .github/scripts/smoke-macos.sh target/release/bundle/dmg/*.dmg
pwsh .github/scripts/smoke-windows.ps1 -Kind nsis -Installer <setup.exe>
```

The same jobs run the user-facing installer against the fresh packages (`install.sh --from <dir>`, `install.ps1 -From <dir>`) and then uninstall with it, so the path a user takes is the path that is tested.

All of this runs on every push through the workflows in `.github/workflows/`. A tag `v*` builds the installers for all platforms, uploads them as a draft release with `SHA256SUMS.txt`, and the install scripts read that list.

## Adding a provider

1. Find the vendor's documented usage endpoint. If there is none, stop: the app does not scrape dashboards or call private APIs. Record the URL, method, auth scheme, response fields and whether the vendor says the call is free in `docs/SOURCES.md`, with the date.
2. Copy the vendor's documented example responses, verbatim, into `crates/core/tests/fixtures/` and list them in the README there.
3. Add `crates/core/src/providers/<name>.rs` implementing `Provider`. `firecrawl.rs` is the plain Bearer case, `serpapi.rs` the query-parameter case, `runway.rs` the extra-header case. Deserialize only the numbers you need, and leave out anything the vendor echoes back (names, emails, key labels). If the response carries identifiers, add the struct to `providers_do_not_deserialize_echoed_identity_fields` in `crates/core/tests/source_audit.rs`.
4. Add it to `registry()` in `providers/mod.rs`, update the id list in that file's test and the provider count in `.github/e2e/drive.py`. The window picks it up on its own.
5. Add the host to the lists in `SECURITY.md`, `docs/THREAT_MODEL.md` and `.github/repository-metadata.yml`, and a row to the README table.
6. If the key has a documented prefix, add it to `KEY_PREFIXES` in `crates/core/src/redact.rs` with its source.

## Release

```sh
git tag v0.2.0 && git push origin v0.2.0
```

The release workflow builds macOS (Apple silicon and Intel), Windows and Linux, uploads everything to a draft release, and appends `SHA256SUMS.txt`. Review the draft on GitHub and publish it. The install scripts fetch the latest published release, so a draft is invisible to users until you publish.
