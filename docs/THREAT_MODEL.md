# Threat model

Last reviewed 2026-10-16. Update this file in the same commit as any change that moves a trust boundary.

## What the app protects

1. API keys for Firecrawl, SerpApi, Elicit, Context7, Tavily, OpenRouter, Apify, Hunter and Runway. A leaked key lets someone spend the user's credits.
2. Account details the vendors echo back. SerpApi's account call returns the key and the account email in its response body. Hunter's returns the owner's name and email. OpenRouter's returns the first and last characters of the key as a label, plus user and workspace identifiers. The parsers leave these fields out of their structs, and `crates/core/tests/source_audit.rs` fails the build if a struct names one of them.
3. Usage numbers. They are low value, but no record of them should outlive the day.

## Trust boundaries

| Component | Trusted to hold secrets? | Why |
| --- | --- | --- |
| The logged-in OS user account | Yes | If an attacker runs code as this user, the OS keystore will hand them the keys. The app cannot prevent that. |
| OS credential store (Keychain, Credential Manager, Secret Service) | Yes | This is where keys live at rest. It is the only place they are written. |
| The app's Rust process memory | Briefly | A key is read from the store at fetch time, wrapped in `secrecy::SecretString`, used for one request and dropped (zeroized). It is never cached in app state. |
| The app's logs | No | Every log line passes through `redact.rs` before it reaches stderr. The default level is `warn`. |
| Crash dumps | No | On Linux the process marks itself non-dumpable and sets the core-file limit to zero at startup. |
| The settings file | No | Holds poll interval, enabled providers and theme only. A test fails the build if a secret-like field name ever appears in it. |
| The webview (HTML/JS) | No | It sees a key once, in the input field, at paste time. It never gets a key back from Rust. It cannot make network requests (`connect-src 'none'`. Tauri's IPC falls back from its custom-protocol `fetch` to `postMessage`, which CSP does not govern) and can only call the commands listed in `src-tauri/capabilities/main.json`. |
| The network | No | TLS 1.2 or newer through rustls with the ring provider. Redirects are disabled. A per-provider host allowlist is checked before every request. |
| Vendor responses | No | Bodies are capped at 64 KiB, parsed into structs that leave out echoed secrets, and the raw buffer is zeroized. Vendor error strings are stripped of control characters, truncated, scrubbed and shown with `textContent`, never as HTML. |

## Attacks considered

| Attack | Control | Residual risk |
| --- | --- | --- |
| Key leaks into a log line or error message | `redact.rs` strips URL query strings, `api_key=` pairs, bearer tokens and long key-shaped tokens from every log line. Transport errors are built from `reqwest::Error::without_url()`. A test logs a fake SerpApi request and asserts that the key is absent. | A new log call that formats a key in a shape the scrubber does not recognise. Code review covers this. |
| Key leaks into a panic message | No `unwrap`, `expect` or `panic!` outside tests (Clippy denies them). The panic hook prints only a scrubbed message and the source location. | Panics inside dependencies can still carry their own payloads. They pass through the same hook. |
| Key leaks into freed heap memory | `SecretString` zeroizes on drop. Intermediate buffers (`Bearer ...` header text, response bodies) are `Zeroizing`. Body buffers are allocated once at full size, so they never reallocate and leave old copies behind. `tests/leak.rs` installs a scanning allocator and fails if secret bytes are found in any freed block. | Copies made inside reqwest/hyper (header `Bytes`, the request `Url` for SerpApi, socket buffers), inside serde_json while Tauri decodes the IPC call, and inside the platform credential-store client. See "Known residual risks". |
| Key leaks to disk | The app writes no key, email or usage number to its own files. There is no cache, database or history file. The webview runs in incognito mode. On Linux, where WebKitGTK still creates `hsts-storage.sqlite` and salt files in its data directory even in incognito mode (observed 2026-10-16), that directory is a per-launch folder in `$XDG_RUNTIME_DIR` deleted on exit. The installer smoke tests fail if the app leaves any file other than `settings.json`. | The OS may page process memory to swap. Use encrypted swap if that matters to you. On Windows, WebView2 keeps a working folder in `%LOCALAPPDATA%\<identifier>` (browser-engine state, no app data). |
| Hostile webview content (XSS) | Strict CSP: no inline scripts, no remote origins, `object-src 'none'`. `withGlobalTauri` is false. Prototype freezing is on. All vendor text goes into `textContent`. Devtools are compiled out of release builds. A navigation guard refuses any page outside the app's own origin, so a remote page can never run with the app's IPC (checked end to end). | A 0-day in the system webview. |
| Unclean shutdown leaves state behind | SIGTERM, SIGINT and window close all go through Tauri's exit path: history wiped, webview folder deleted, exit code 0. In AppImage extract-and-run mode the runtime stays as the parent and forwards no signals, so the app sets `PR_SET_PDEATHSIG` and follows the runtime out. Installer smoke tests check exit codes, leftover helper processes and leftover files on Linux, macOS and Windows. | `SIGKILL` and power loss skip the exit path. The Linux webview folder then sits in `$XDG_RUNTIME_DIR` until the next launch sweeps it or the session ends. |
| Frontend calls a command it should not | `tauri-build`'s `AppManifest::commands` restricts the app's command set. The capability file grants each command by name, plus event listening. No HTTP, shell, filesystem or opener plugin is linked. | None known. |
| Network attacker (MITM) | rustls with the platform verifier, TLS 1.2 minimum, `https_only`, redirects off, host allowlist. | A CA the user's OS trusts that issues a fraudulent certificate. The app trusts the OS trust store, as the OS does. |
| Redirect or DNS trick sends the key to another host | Redirects are disabled. The URL host is checked against the provider's allowlist before the request is built. | DNS hijack to an attacker host that also holds a valid certificate for the vendor's name. This collapses into the CA case above. |
| Malicious dependency | Exact version pins, committed lockfiles, `cargo-deny` (licenses, duplicate crypto crates, advisories), `cargo-audit`, `npm audit`, npm install scripts disabled through `.npmrc`, GitHub Actions pinned to commit SHAs. | A compromised release of a pinned crate published before an advisory exists. |
| Tampered release binary | SHA-256 sums are published next to each release artifact. | The macOS build is unsigned, so Gatekeeper cannot vouch for it. Build from source if you need assurance. |

## Known residual risks

These are copies of a secret that the app cannot zeroize because they live in code it does not own. They are listed here so nobody mistakes the controls above for a guarantee.

1. The webview. When you paste a key, the JavaScript engine holds the string. JS strings are immutable and garbage-collected. The app clears the input field, but it cannot wipe the engine's copy.
2. Tauri IPC decoding. The `save_key` argument arrives as JSON. serde_json builds intermediate values that are freed without zeroizing.
3. HTTP stack. reqwest copies the `Authorization` header into a `HeaderValue` (marked sensitive, so it is not logged or HPACK-indexed) and hyper copies it into socket buffers. For SerpApi, the key also sits in the request `Url`, because SerpApi only accepts it as a query parameter. SerpApi also echoes the key back in the response body, and hyper's receive buffers hold that copy.
4. Credential-store client. On Linux, keyring talks to the Secret Service over D-Bus through zbus, using an encrypted session (`crypto-rust`). The plaintext still passes through zbus and the crypto code before encryption. The automated leak test checks the app's own wrapper against an in-memory store. The core-dump leak test in `.github/scripts/leak-test-linux.sh` checks the full path against the real daemon. Results are recorded in `docs/SOURCES.md`.
5. Same-user attacker. Anyone running code as you can ask the keystore for the keys, read this process's memory (except on Linux, where the non-dumpable flag also blocks `ptrace` attach by processes without `CAP_SYS_PTRACE`), or install a keylogger. This is outside the model by design.

## Linux without a Secret Service daemon

If no Secret Service provider (GNOME Keyring, KWallet, KeePassXC) is running, the app shows an error explaining how to install and unlock one. It does not fall back to the kernel keyutils store or to a file, because a silent fallback would change where keys live without the user knowing.
