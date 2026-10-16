# Follow-ups

Ideas, known gaps and nearby problems, recorded here instead of implemented. Anything that adds persistence or a network call needs a design review in this file first and must be opt-in, off by default.

## Needs the owner

1. Verify live. Run the app with real Firecrawl, SerpApi and Elicit keys and record the date and the fields you saw in `docs/SOURCES.md` under "Verified live". Until then every provider is "Verified" against documentation only.
2. Firecrawl cost. With a live key and no other usage, compare `remainingCredits` across two polls. If it does not move, the 30-minute floor in `providers/firecrawl.rs` can drop to the global default. Firecrawl's billing page does not list this endpoint.
3. Elicit cost. Same question for `GET /usage`. The docs are silent.
4. Self-updater (not in v0.1). Tauri's updater plugin needs a signing key pair. The owner has to generate it, keep the private key offline or in a CI secret, and publish the public key in `tauri.conf.json`. Until then users update by downloading a new release and checking its SHA-256 sum.
5. macOS signing and notarization. The macOS build is unsigned, so Gatekeeper warns. Signing needs an Apple Developer ID, which is a cost and an account decision.
6. Windows signing. Same, with a code-signing certificate. SmartScreen will warn on unsigned builds.

## Security hardening worth a design review

7. Tauri isolation pattern. Tauri can route IPC through a sandboxed isolation iframe that validates every message. The app has eight commands and a strict CSP, so the gain is small, but it is cheap defense in depth.
8. Raw IPC body for `save_key`. The key currently crosses the bridge as a JSON argument, and serde_json's intermediate values are freed without zeroizing (see THREAT_MODEL "Known residual risks"). A raw-bytes command body would let Rust zeroize its copy, though Tauri still owns the original buffer. Measure with the leak test before deciding.
9. SerpApi URL copy. reqwest keeps the request URL, key included, for the life of the request, and frees it unzeroized. This can only be fixed upstream in reqwest/hyper or by moving to a lower-level client.
10. Windows crash dumps. Windows Error Reporting can capture process dumps. Opting the process out (`WerAddExcludedApplication`, or `SetErrorMode`) needs a small amount of `unsafe` FFI and testing on a Windows machine.
11. macOS `ptrace` denial. `PT_DENY_ATTACH` is the macOS counterpart to Linux's non-dumpable flag. It needs testing alongside debug builds, because it also blocks legitimate debuggers.

## Dependencies

12. GTK3 advisories. `RUSTSEC-2024-0370` (proc-macro-error unmaintained) and `RUSTSEC-2024-0429` (glib `VariantStrIter` unsound) come from gtk-rs 0.18 through Tauri 2's Linux webview. Both are ignored in `deny.toml` and `.cargo/audit.toml` with reasons. Remove the ignores when Tauri moves to GTK4 or gtk-rs publishes fixes.
13. `target-lexicon` license exception. "Apache-2.0 WITH LLVM-exception" for a build-time-only crate. Keep an eye on whether it ever becomes a runtime dependency.

## Features

14. More providers. Each is one file in `crates/core/src/providers/` plus one line in `registry()`. Candidates must have a documented usage endpoint. See the README section "Adding a provider".
15. Low-balance notification. A desktop notification when a provider drops below 20%. It needs the notification plugin and a capability for it. Off by default.
16. Per-provider poll interval. Today there is one global interval with a per-provider floor.
17. History beyond today. Explicitly out of scope: usage history is wiped at midnight by design. Any change needs the design review described at the top of this file.
