# Follow-ups

Ideas, known gaps and nearby problems, recorded here instead of implemented. Anything that adds persistence or a network call needs a design review in this file first and must be opt-in, off by default.

## Needs the owner

1. Verify live. Run the app with real Firecrawl, SerpApi and Elicit keys and record the date and the fields you saw in `docs/SOURCES.md` under "Verified live". Until then every provider is "Verified" against documentation only.
2. Firecrawl cost, live check. The open-source API server shows the credit-usage call does not bill (docs/SOURCES.md, Firecrawl row, with the commit hash), so the 30-minute floor is gone. With a live key and no other usage, compare `remainingCredits` across two polls to confirm against the hosted service.
3. Cost of the usage call at Elicit, Tavily, OpenRouter, Apify and Runway. Their docs are silent (Tavily's pricing page lists other calls only). With a live key and no other usage, poll twice and compare. SerpApi, Context7 and Hunter document theirs as free.
4. Self-updater (not in v0.1). Tauri's updater plugin needs a signing key pair. The owner has to generate it, keep the private key offline or in a CI secret, and publish the public key in `tauri.conf.json`. Until then users update by downloading a new release and checking its SHA-256 sum.
5. macOS signing and notarization. The macOS build is unsigned, so Gatekeeper warns. Signing needs an Apple Developer ID, which is a cost and an account decision.
6. Windows signing. Same, with a code-signing certificate. SmartScreen will warn on unsigned builds.
7a. Make the repository findable. It is private, so GitHub search, search engines and GitHub Pages (on the free plan) cannot see it. After deciding to make it public: set the description, homepage and 20 topics from `.github/repository-metadata.yml` (the session's proxy does not allow settings writes, so this was not done automatically), turn on Pages from `main` / `docs`, upload `docs/assets/social-preview.png` under Settings > General > Social preview (no API exists for it), and turn on private vulnerability reporting so the link in `SECURITY.md` works.

## Security hardening worth a design review

7. Tauri isolation pattern. Tauri can route IPC through a sandboxed isolation iframe that validates every message. The app has eight commands and a strict CSP, so the gain is small, but it is cheap defense in depth.
8. Raw IPC body for `save_key`. The key currently crosses the bridge as a JSON argument, and serde_json's intermediate values are freed without zeroizing (see THREAT_MODEL "Known residual risks"). A raw-bytes command body would let Rust zeroize its copy, though Tauri still owns the original buffer. Measure with the leak test before deciding.
9. SerpApi URL copy. reqwest keeps the request URL, key included, for the life of the request, and frees it unzeroized. This can only be fixed upstream in reqwest/hyper or by moving to a lower-level client.
10. Windows crash dumps. Windows Error Reporting can capture process dumps. Opting the process out (`WerAddExcludedApplication`, or `SetErrorMode`) needs a small amount of `unsafe` FFI and testing on a Windows machine.
11. macOS `ptrace` denial. `PT_DENY_ATTACH` is the macOS counterpart to Linux's non-dumpable flag. It needs testing alongside debug builds, because it also blocks legitimate debuggers.

## Dependencies

12. GTK3 advisories. `RUSTSEC-2024-0370` (proc-macro-error unmaintained) and `RUSTSEC-2024-0429` (glib `VariantStrIter` unsound) come from gtk-rs 0.18 through Tauri 2's Linux webview. Both are ignored in `deny.toml` and `.cargo/audit.toml` with reasons. (`deny.toml` only started seeing them once the review set `unmaintained` and `unsound` to `"all"`. Before that cargo-deny checked direct dependencies only.) Remove the ignores when Tauri moves to GTK4 or gtk-rs publishes fixes.
13. `target-lexicon` license exception. "Apache-2.0 WITH LLVM-exception" for a build-time-only crate. Keep an eye on whether it ever becomes a runtime dependency.

## Features

14. More providers. Each is one file in `crates/core/src/providers/` plus one line in `registry()` (docs/DEVELOPMENT.md, "Adding a provider"). Looked at and not added, with reasons in docs/SOURCES.md ("Vendors looked at and not added"): Exa (usage needs a team service key), ElevenLabs (documented endpoint, no example response to use as a fixture; ask the vendor or capture a live response and record it), Bright Data (no documented balance endpoint found), Zyte (a Stats API exists, unread), Browser Use and You.com (nothing found).
15. Low-balance notification. A desktop notification when a provider drops below 20%. It needs the notification plugin and a capability for it. Off by default.
16. Per-provider poll interval. Today there is one global interval with a per-provider floor.
17. History beyond today. Explicitly out of scope: usage history is wiped at midnight by design. Any change needs the design review described at the top of this file.

## From the 2026-10-16 review (details in docs/REVIEW.md)

18. Resolved after the review: incognito is off on Linux only, where the per-launch data directory in `$XDG_RUNTIME_DIR` already gives a throwaway store, and on for macOS and Windows. The e2e job passes 17 of 17 on the committed code (docs/SOURCES.md, 2026-10-16). Original finding: docs/REVIEW.md, finding 9.
19. Webview data on Windows and macOS. The owner's rule is that the settings file is the only file written. WebView2 creates a user data folder under `%LOCALAPPDATA%\<identifier>` even with incognito, and WKWebView may create `~/Library/Caches/<identifier>` and `~/Library/WebKit/<identifier>`. Point `data_directory` at a per-launch temp folder on Windows too and delete it on exit (WebView2 must have released it first), and measure what macOS leaves. The README now states the current behaviour.
20. Linux webview folder fallback. When `XDG_RUNTIME_DIR` is unset, `window::linux::base()` uses the shared temp folder with a predictable name (`mr-usage-webview-<pid>`). Another local user can pre-create that name and make startup fail, and the folder is on disk rather than tmpfs. Use a random suffix (or refuse to start without `XDG_RUNTIME_DIR`).
21. Resolved by the Firecrawl finding above: the floor is now one minute for every provider, the same as the manual-refresh cooldown, so the note and the button agree.
22. History cap versus "today's readings". `HISTORY_CAP` is 288 samples, one day at 5-minute polling. At the 1-minute interval SerpApi and Elicit allow, the sparkline covers under five hours, not the whole day the README describes. Size the buffer from the provider's interval or change the wording.
23. Changing the poll interval does not reschedule. `apply_settings` keeps each provider's existing `next_due`, so moving from 1 hour to 1 minute waits up to an hour for the next poll.
24. Redaction gaps that need a design decision. The scrubber does not percent-decode (`api%5Fkey%3D<short value>` passes), a marker followed by a newline stops protecting the value on the next line, and a short value with no marker and no key shape passes. None of these affect the three current providers' key formats. Decide whether to percent-decode before scrubbing.
25. cargo-deny-action image. The pinned action builds a Dockerfile that downloads the cargo-deny binary with `curl` and no checksum, and runs `apk update` unpinned. Pinning the action SHA does not pin what that build fetches. Consider installing cargo-deny with `cargo install --locked` as is done for cargo-audit.
26. Release job hygiene. The release build restores npm and Rust caches written by `main` and does not run the test, audit or deny jobs before producing artifacts. Consider a cache-free release build that `needs` the CI checks.
27. Settings file write. `Settings::save` writes `settings.json.tmp` with default permissions and follows a pre-existing symlink at that path. It also does not fsync. Low risk (no secrets in the file), but `O_EXCL` plus a random temp name would be tidier.
28. source_audit coverage. `exposed_secrets_never_reach_format_or_log_macros` only looks at the line containing `expose_secret`, so a multi-line `format!(` call is not caught, and `without_tests` drops everything after the first `#[cfg(test)]\nmod tests` in a file.
29. Smoke-test blind spots. `.github/scripts/smoke-linux.sh`: the AppImage "uninstall" check deletes the file and then checks that the file is gone, which cannot fail. `helpers()` counts every WebKit process on the machine, not just the app's. The "wrote nothing" check scans `$HOME` only, so files in the temp folder or `XDG_RUNTIME_DIR` (other than the webview folder) go unnoticed. `.github/scripts/smoke-windows.ps1` does not check the NSIS installer's or uninstaller's exit code. `.github/e2e/run-linux.sh` does not notice when `tauri-driver` fails to bind port 4444 and will talk to whatever already listens there.

## From the provider and installer work (2026-10-16)

30. Signed builds. The install scripts verify a download against the release's `SHA256SUMS.txt`, which proves the file is the one the release workflow uploaded, not who built it. Apple and Microsoft signing (items 5 and 6) would let Gatekeeper and SmartScreen do that check themselves and remove the quarantine and mark-of-the-web steps from the scripts.
31. System uninstallers and keys. "Remove program" on Windows, dragging the app to the Trash on macOS and `apt remove` leave the keystore entries alone. `install.sh --uninstall` and `install.ps1 -Uninstall` run `mr-usage --forget-keys` first. Tauri's NSIS `installerHooks` (`NSIS_HOOK_PREUNINSTALL`) could run the same command from the system uninstaller; the deb's `preRemoveScript` could too, if a user session's Secret Service were reachable from a root postrm, which it usually is not.
32. End-to-end job on CI. The `e2e-linux` job failed on commit 9d51496 while the same script passes locally (17 of 17). The job log and the screenshot artifact are served from a storage host this sandbox cannot reach, so the cause is unknown. `run-linux.sh` now repeats the failed checks and the tail of the app log as GitHub annotations, which the API does serve. Read them on the next run.
33. Install scripts against a published release. CI exercises `install.sh --from` and `install.ps1 -From` on the freshly built packages, and the checksum refusal path was tested by hand. The download path (`api.github.com`, asset selection by suffix, `SHA256SUMS.txt` from the release) can only be tested once a `v*` tag has produced a published release.
34. Card wording for money. OpenRouter and Apify show dollars (`Unit::Usd`, whole cents inside the app). The sparkline tooltip shows the same formatting. Check the layout with a six-figure cent value.
35. Context7 on pay-as-you-go. `includedInPlan` is 0, so the card shows "No remaining count on this plan" and the month's request count and extra cost in the facts list. Someone on that plan should say whether that is the right headline.

