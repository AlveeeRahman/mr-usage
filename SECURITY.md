# Security

Mr. Usage holds API keys that can spend your money, so this page says what it does with them, what it does not do, and how to report a problem.

## Reporting a vulnerability

Please report privately through GitHub: open the repository's Security tab and choose "Report a vulnerability". Do not open a public issue for anything that could expose keys. Include the version, your OS, and the steps to reproduce.

## What the app does and does not do

- Writes keys only to the OS credential store (macOS Keychain, Windows Credential Manager, the Secret Service on Linux), under the service name `mr-usage`. It never writes a key to its own files.
- Reads a key at the moment it calls the vendor, wraps it in `secrecy::SecretString`, and drops it, which zeroizes it. Keys are never cached in app state and never sent back to the window.
- Contacts only `api.firecrawl.dev`, `serpapi.com` and `elicit.com`, over TLS 1.2 or newer, HTTPS only, with redirects disabled and a host allowlist checked before every request. No telemetry, no update check.
- Writes one file of its own, `settings.json` (poll interval, enabled providers, theme). On Linux the webview's scratch folder lives in `$XDG_RUNTIME_DIR` and is deleted on exit. On macOS and Windows the system webview keeps its own folder, and the README lists the paths.
- Runs its window under a strict Content Security Policy, with no network access from the page, devtools compiled out of release builds, and exactly eight commands the page may call.
- Cannot protect keys from someone who can run programs as you. The OS keystore hands keys to processes running under your account.

The full design, including the copies of a key the app cannot zeroize because they live in libraries it does not own, is in [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md). An independent review is in [docs/REVIEW.md](docs/REVIEW.md).

## What is checked on every change

| Gate | Command |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` |
| Lints, every warning an error | `cargo clippy --workspace --all-targets -- -D warnings` (no `unwrap`, `expect`, `panic!` or print macros outside tests) |
| Tests on macOS, Windows and Linux | `cargo test --workspace`, including the log-redaction, settings and allocator leak tests |
| Real keystore and leak tests | `cargo test -p mr-usage-core -- --ignored` and `scripts/leak-test-linux.sh` against GNOME Keyring |
| End-to-end | `e2e/run-linux.sh` drives the real app through WebDriver |
| Licenses, bans, advisories | `cargo deny check`, `cargo audit -D warnings`, `npm audit --audit-level=low` |
| Installers | `scripts/smoke-{linux,macos}.sh`, `scripts/smoke-windows.ps1`: install, start, stop three ways, uninstall |

GitHub Actions are pinned to commit SHAs and lockfiles are committed.
