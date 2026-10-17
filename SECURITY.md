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

## Verifying a download

Every release carries `SHA256SUMS.txt`, a Sigstore signature of that list (`SHA256SUMS.txt.sigstore.json`, made with the release workflow's own OIDC identity, so there is no signing key to leak), and a GitHub build-provenance attestation for each installer. The install scripts check the SHA-256 for you. To check the rest by hand:

```sh
# The signature: the list was produced by this repository's release or sign-release workflow.
cosign verify-blob --bundle SHA256SUMS.txt.sigstore.json \
  --certificate-identity-regexp '^https://github\.com/AlveeeRahman/mr-usage/\.github/workflows/(release|sign-release)\.yml@refs/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  SHA256SUMS.txt
sha256sum -c --ignore-missing SHA256SUMS.txt

# The provenance: this installer was built by the release workflow in this repository.
gh attestation verify Mr.Usage_1.0.0_x64.dmg --repo AlveeeRahman/mr-usage
```

Releases published before v1.0.1 were signed afterwards with the `sign-release` workflow and carry the signature but no provenance.

## Known advisories

`cargo audit` runs with every advisory as an error. The ones it is told to ignore are listed with their reasons in [.cargo/audit.toml](.cargo/audit.toml) and tracked in [docs/FOLLOWUPS.md](docs/FOLLOWUPS.md); at the time of writing two come from the GTK 3 bindings Tauri uses on Linux and three from the zbus 5 crates behind the Linux keyring store, and none is reachable from this app's code. [osv-scanner.toml](osv-scanner.toml) carries the same list with the same reasons, so the [OpenSSF Scorecard report](https://scorecard.dev/viewer/?uri=github.com/AlveeeRahman/mr-usage) does not count them under Vulnerabilities; every other finding there stands. Code scanning (CodeQL and Scorecard) and Dependabot alerts are on the repository's [Security tab](https://github.com/AlveeeRahman/mr-usage/security), and Dependabot opens a grouped pull request each Monday for Cargo, npm and Actions updates ([.github/dependabot.yml](.github/dependabot.yml)).

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
| Fuzzing | `cargo +nightly fuzz run` on every provider parser, the error-body reader and the redaction rules, a minute each under AddressSanitizer |
| Installers | `.github/scripts/smoke-{linux,macos}.sh`, `.github/scripts/smoke-windows.ps1`: install, start, stop three ways, uninstall, then `installers/install.sh` or `installers/install.ps1` against the same packages |
| Repository practices | [OpenSSF Scorecard](https://scorecard.dev/viewer/?uri=github.com/AlveeeRahman/mr-usage) on every push to `main` and weekly; CodeQL on every push and pull request |

GitHub Actions are pinned to commit SHAs and lockfiles are committed. `main` takes only pull requests whose CI and installer checks passed; force pushes and deletion are blocked by a repository ruleset.
