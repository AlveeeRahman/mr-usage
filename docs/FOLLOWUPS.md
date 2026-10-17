# Known gaps and planned work

Recorded here instead of implemented. Anything that adds persistence or a network call is opt-in and off by default.

## Needs a live account

1. Live verification. Every provider is verified against vendor documentation. Run the app with a real key for each and record the date and the fields seen in `docs/SOURCES.md` under "Verified live".
2. Cost of the usage call. Firecrawl's open-source API server shows the credit-usage call does not bill. Confirm against the hosted service by comparing `remainingCredits` across two polls with no other usage. Elicit, Tavily, OpenRouter, Apify and Runway do not say, so poll twice and compare. SerpApi, Context7 and Hunter document their call as free.

## Release and distribution

3. Code signing. The macOS build is not signed or notarized and the Windows build is not signed, so Gatekeeper and SmartScreen warn and the install scripts clear the quarantine flag and the mark-of-the-web after verifying the checksum. An Apple Developer ID and a Windows code-signing certificate would let the operating systems do that check themselves.
4. Self-updater. Tauri's updater plugin needs a signing key pair, the private key kept offline or in a CI secret and the public key in `tauri.conf.json`. Until then users rerun the install command.
5. System uninstallers and keys. "Remove program" on Windows, dragging the app to the Trash on macOS and `apt remove` leave the keystore entries alone. `install.sh --uninstall` and `install.ps1 -Uninstall` run `mr-usage --forget-keys` first. Tauri's NSIS `NSIS_HOOK_PREUNINSTALL` could run the same command from the system uninstaller. A deb `preRemoveScript` runs as root and cannot reach a user's Secret Service.
6. Anonymous download path of the install scripts. CI covers `--from` against fresh packages and `--version` against a draft release. The `releases/latest` lookup with public download URLs can only be exercised against a published release. Run `install.sh` once after the first publish and record it in `docs/SOURCES.md`.
7. Repository settings. Description, homepage, topics, the social preview image and private vulnerability reporting are set by hand from `.github/repository-metadata.yml`: GitHub has no API for the preview image, and the others need administration rights that a workflow token does not have. GitHub Pages needs one click (Settings, Pages, Source: GitHub Actions); after that `.github/workflows/pages.yml` publishes it on every change to `docs/`.

## Security hardening

8. Tauri isolation pattern. IPC can be routed through a sandboxed isolation iframe that validates every message. With eight commands and a strict CSP the gain is small, but it is cheap defense in depth.
9. Raw IPC body for `save_key`. The key crosses the bridge as a JSON argument and serde_json's intermediate values are freed without zeroizing (`docs/THREAT_MODEL.md`, "Known residual risks"). A raw-bytes command body would let Rust zeroize its copy. Measure with the leak test before deciding.
10. SerpApi URL copy. reqwest keeps the request URL, key included, for the life of the request and frees it unzeroized. Fixable only upstream or by moving to a lower-level client.
11. Windows crash dumps. Windows Error Reporting can capture process dumps. Opting out (`WerAddExcludedApplication` or `SetErrorMode`) needs a small amount of `unsafe` FFI and testing on Windows.
12. macOS `ptrace` denial. `PT_DENY_ATTACH` is the macOS counterpart to Linux's non-dumpable flag. It also blocks legitimate debuggers, so it needs testing alongside debug builds.
13. Webview data on Windows and macOS. WebView2 creates a user data folder under `%LOCALAPPDATA%\<identifier>` even with incognito, and WKWebView may create `~/Library/Caches/<identifier>` and `~/Library/WebKit/<identifier>`. Point `data_directory` at a per-launch temp folder on Windows too and delete it on exit, and measure what macOS leaves.
14. Linux webview folder fallback. When `XDG_RUNTIME_DIR` is unset the app uses the shared temp folder with a predictable name (`mr-usage-webview-<pid>`), which another local user can pre-create. Use a random suffix or refuse to start.
15. Redaction gaps. The scrubber does not percent-decode (`api%5Fkey%3D<short value>` passes), a marker followed by a newline stops protecting the value on the next line, and a short value with no marker and no key shape passes. None affect the current providers' key formats.
16. Source audit coverage. `exposed_secrets_never_reach_format_or_log_macros` looks only at the line containing `expose_secret`, so a multi-line `format!(` call is not caught, and `without_tests` drops everything after the first `#[cfg(test)]\nmod tests` in a file.

## Dependencies

17. GTK3 advisories. `RUSTSEC-2024-0370` (proc-macro-error unmaintained) and `RUSTSEC-2024-0429` (glib `VariantStrIter` unsound) come from gtk-rs 0.18 through Tauri 2's Linux webview and are ignored in `deny.toml` and `.cargo/audit.toml` with reasons. Remove the ignores when Tauri moves to GTK4 or gtk-rs publishes fixes.
18. `target-lexicon` license exception. "Apache-2.0 WITH LLVM-exception" for a build-time-only crate. Watch whether it ever becomes a runtime dependency.
19. cargo-deny-action image. The pinned action builds a Dockerfile that downloads the cargo-deny binary with `curl` and no checksum and runs `apk update` unpinned. Consider `cargo install --locked` as is done for cargo-audit.

## Behaviour

20. History cap. `HISTORY_CAP` is 288 samples, one day at 5-minute polling. At the 1-minute interval the sparkline covers under five hours. Size the buffer from the interval or change the wording.
21. Money formatting. OpenRouter and Apify show dollars (`Unit::Usd`, whole cents inside the app). Check the card layout with a six-figure cent value.
22. Context7 on pay-as-you-go. `includedInPlan` is 0, so the card shows "No remaining count on this plan" with the month's request count and extra cost in the facts list.
23. Low-balance notification. A desktop notification under 20%. Needs the notification plugin and a capability for it. Off by default.
24. Per-provider poll interval. There is one global interval with a per-provider floor.
25. History beyond today. Out of scope by design: history is wiped at midnight.

## Tests

26. Smoke-test blind spots. In `smoke-linux.sh` the AppImage "uninstall" check deletes the file and then checks that it is gone. `helpers()` counts every WebKit process on the machine. The "wrote nothing" check scans `$HOME` only. `smoke-windows.ps1` does not check the NSIS installer's or uninstaller's exit code. `run-linux.sh` does not notice when `tauri-driver` fails to bind port 4444.
27. Release build hygiene. The release build restores caches written by `main` and does not `need` the test, audit or deny jobs. Consider a cache-free release build gated on them.

## Providers considered and not added

28. Exa: usage is only in the Team Management API, which needs a service key and a key id and is enabled per team. ElevenLabs: `GET /v1/user/subscription` is documented but the page has no example response to use as a fixture. Bright Data, Zyte, Browser Use, You.com, Higgsfield: no documented endpoint returns the account's remaining balance.
