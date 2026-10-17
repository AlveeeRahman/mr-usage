# Security

Mr. Usage holds API keys that can spend your money, so this page says what it does with them, what it does not do, and how to report a problem.

## Reporting a vulnerability

Please report privately through GitHub: open the repository's Security tab and choose "Report a vulnerability". Do not open a public issue for anything that could expose keys. Include the version, your OS, and the steps to reproduce.

## What the app does and does not do

- Writes keys only to the OS credential store (macOS Keychain, Windows Credential Manager, the Secret Service on Linux), under the service name `mr-usage`. It never writes a key to its own files.
- Reads a key at the moment it calls the vendor, wraps it in `secrecy::SecretString`, and drops it, which zeroizes it. Keys are never cached in app state and never sent back to the window.
- Contacts only the documented usage host of each provider you added a key for (one host per provider, listed under `network_surfaces` in `.github/repository-metadata.yml`), over TLS 1.2 or newer, HTTPS only, with redirects disabled and a host allowlist checked before every request. No telemetry, no update check. (The install scripts, which run outside the app, read GitHub Releases.)
- Writes one file of its own, `settings.json` (poll interval, enabled providers, theme). On Linux the webview's scratch folder lives in `$XDG_RUNTIME_DIR` and is deleted on exit. On macOS and Windows the system webview keeps its own folder, and the README lists the paths.
- Runs its window under a strict Content Security Policy, with no network access from the page, devtools compiled out of release builds, and exactly ten commands the page may call.
- Deletes its own keystore entries when asked (`mr-usage --forget-keys`), which the install scripts do before uninstalling. `--help` and `--version` are the only other options. An unrecognised argument stops the app with exit status 2 before anything is initialised.
- Cannot protect keys from someone who can run programs as you. The OS keystore hands keys to processes running under your account.

The full design, including the copies of a key the app cannot zeroize because they live in libraries it does not own, is in [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).

## What is checked on every change

| Gate | Command |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` |
| Lints, every warning an error | `cargo clippy --workspace --all-targets -- -D warnings` (no `unwrap`, `expect`, `panic!` or print macros outside tests) |
| Tests on macOS, Windows and Linux | `cargo test --workspace`, including the log-redaction, settings, allocator leak and command-line tests |
| Minimum Rust version | `cargo +1.90 check -p mr-usage-core --locked --all-targets` |
| Real keystore and leak tests | `cargo test -p mr-usage-core -- --ignored` and `.github/scripts/leak-test-linux.sh` against GNOME Keyring |
| End-to-end | `.github/e2e/run-linux.sh` drives the real app through WebDriver |
| Licenses, bans, advisories | `cargo deny check`, `cargo audit -D warnings`, `npm audit --audit-level=low` |
| Installers | `.github/scripts/smoke-{linux,macos}.sh`, `.github/scripts/smoke-windows.ps1`: install, start, stop three ways, uninstall, then `installers/install.sh` or `installers/install.ps1` against the same packages |

GitHub Actions are pinned to commit SHAs and lockfiles are committed.
