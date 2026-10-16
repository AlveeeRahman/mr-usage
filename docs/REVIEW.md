# Security and correctness review

Reviewed 2026-10-16 against commit `834a94d` (the five commits on the branch at the time). The reviewer had not seen the code being written and judged only what is in the repository. Every file was read. The frontend, Rust crates, workflows, scripts, e2e driver and docs were all in scope.

Line numbers point to the file as it stands after the review commits. Where a fix moved code, the description says what the old code did.

Severity scale: high means a key can leak or a stated control does not exist. Medium means a control has a real bypass, a correctness bug with user-visible effect, or a documented claim the code does not back. Low means defense in depth, test-quality or hygiene. Info means worth knowing, no action needed now.

No high-severity finding. The core controls (keystore-only storage, no key back to the webview, allowlist before send, redirects off, rustls with TLS 1.2 minimum, no Debug on secret holders, strict CSP, no plugins) are present and tested.

## Findings

### 1. Long-token rule skipped any word containing `/` (medium, fixed)

`crates/core/src/redact.rs:158` (`looks_like_key`). The old rule required `!word.contains('/')`, and `/` is a word character for the tokenizer. So a key anywhere after a slash was never redacted: `https://elicit.com/api/v2/<64-hex>` and `path/<64-hex>` came out unchanged, and so did `x/fc-<32 hex>` because the prefix check only looked at the start of the whole word.

Failure scenario: a future provider, a dependency's debug log, or a vendor error that puts a key in a URL path (or in any `a/b` token) reaches stderr or a card with the key intact. `redact_url` keeps the path by design, so the token pass was the only line of defence for it.

Fix: each `/`-separated segment is checked on its own, for both the prefix rule and the long-token rule. A file path made of short segments (`/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0`) still survives. Test `keys_after_a_slash_are_scrubbed`.

### 2. `vendor_text` truncated before it scrubbed (medium, fixed)

`crates/core/src/redact.rs:168`. The old code collapsed whitespace, cut to `max_chars`, then scrubbed. A key that straddled the cut became a fragment shorter than 32 characters with no marker in front of it, which no rule recognises.

Failure scenario: a vendor error such as `Invalid API key <key>` long enough that the key starts within 31 characters of the 200-character limit (or 300 in `net::transport_error`) shows up to 31 key characters on the card. Reproduced during review: 20 hex characters of a SerpApi-shaped key survived.

Fix: scrub first, then truncate. Test `truncation_cannot_leave_a_key_fragment`.

### 3. An auth scheme word ended the redaction (low, fixed)

`crates/core/src/redact.rs:128`, `:25`. After `Authorization`, only the next word was redacted, and `redact_next` was then recomputed from that word. `Bearer` is itself a marker, so `Authorization: Bearer <key>` was fine, but `Authorization: Basic <credential>` (or `Digest`, `Negotiate`, `NTLM`) leaked the credential. `Proxy-Authorization` was not a marker at all (the tokenizer keeps the hyphenated word whole).

Failure scenario: a dependency logs a request with a non-Bearer auth header, or a proxy credential, at debug level.

Fix: added `proxy-authorization` to the markers, and a redacted auth-scheme word keeps the redaction going to the next word. Test `auth_scheme_does_not_end_redaction`.

### 4. Remaining redaction gaps (low, follow-up 24)

`crates/core/src/redact.rs`. Still not handled: percent-encoded markers (`api%5Fkey%3D<short value>`), a marker followed by a newline (the value on the next line is protected only if it is key-shaped), and short values with neither marker nor key shape. None of these can expose the current providers' keys, which are long and key-shaped, so the fix is a design choice (percent-decode before scrubbing or not) and is left to the owner.

### 5. A poll that outlived its key wrote into the new key's state (medium, fixed)

`crates/core/src/poller.rs:388-425` (`poll_one`), `:288` (`key_saved`), `:304` (`key_forgotten`). Both key handlers reset the provider's `Runtime`, including `in_flight = false`, while a poll with the old key could still be waiting on the network (up to 15 s). When that poll finished it cleared `in_flight` and recorded its outcome into the fresh runtime.

Failure scenarios:
- The user replaces a rejected key with a good one. The scheduler starts a second, concurrent poll with the new key (the reset cleared `in_flight`). If the old key's 401 lands last, it sets `last_error = Unauthorized` and `next_due = None`, so the new, valid key is never polled again until a manual refresh, and the card says the key was rejected.
- The user presses Forget key mid-poll. The old account's snapshot is appended to the history after the forget, contradicting "forget the old account's history", and stays in memory until midnight.

Fix: `Runtime` carries a `generation` that both handlers bump. `poll_one` records it when it starts, and a finished poll whose generation no longer matches leaves the state untouched. Test `result_for_a_replaced_key_is_discarded` (fails without the fix, passes with it).

### 6. Settings could reach the poller out of order (low, fixed)

`src-tauri/src/commands.rs:38-55` (`AppState::update`). The settings were saved under the settings lock, but `poller.apply_settings` ran after the lock was released. Two concurrent async commands (two `save_key` calls, or `save_key` and `forget_key`) could save A then B but apply B then A.

Failure scenario: the file and `AppState` list both providers as enabled while the poller's own `enabled` list misses one, so it is never polled until the next settings change.

Fix: `apply_settings` now runs while the settings lock is held. Lock order is settings, then poller state. Nothing takes them the other way round, and the event emit inside is a non-blocking dispatch.

### 7. `core:event:allow-unlisten` granted but never used (low, fixed)

`src-tauri/capabilities/main.json:8`. The owner's rule is that the capability grants exactly what the frontend calls. `src/` calls `listen` once and discards the returned `UnlistenFn`. The built bundle only reaches `plugin:event|unlisten` through that function, and Tauri's injected scripts never call it on their own.

Fix: removed the grant, updated the capability description, and noted in `src/ipc.ts` why the unlisten function must not be called. The review's e2e run (below) confirmed that `listen` and updates still work.

### 8. Billing dates were shown in local time (low, fixed)

`src/main.ts:101`. SerpApi sends `plan_renewal_date` as a bare date, which `provider::parse_date` turns into UTC midnight. The card formatted it in local time, so anyone west of UTC saw the renewal one day early (`2026-11-16` showed as Nov 15 in New York). Firecrawl's `billingPeriodEnd` (`…T23:59:59Z`) showed one day late east of UTC. The Firecrawl period start, a few lines later, was already formatted in UTC.

Fix: period-end dates use the existing UTC formatter, and the now-unused local formatter was removed.

### 9. The Linux e2e test cannot pass with the shipped webview config (medium, resolved after review)

`src-tauri/src/window.rs:26` (`.incognito(true)`), `e2e/run-linux.sh`, `.github/workflows/ci.yml` (`e2e-linux`). With incognito on, wry 0.57 builds an ephemeral WebKit context with automation disabled (`wry/src/webkitgtk/web_context.rs`, `create_context` sets `automation = false`, and the incognito branch never applies Tauri's automation flag). WebKitWebDriver then fails with `session not created: The remote session failed to create a new browsing context`.

Reproduced in this review: `e2e/run-linux.sh` (on alternate ports, because a stale `tauri-driver` from another session held 4444) failed at session creation. With `.incognito(true)` removed locally and nothing else changed, all 17 checks passed, including the new log check from finding 16. The `e2e-linux` CI job should therefore be red on this branch, and the verification log's "17 of 17" row in `docs/SOURCES.md` does not describe the committed code. A row recording this was added to that log.

Not fixed: whether to drop incognito on Linux (the per-launch data directory already gives a throwaway store), gate it on Tauri's automation variable, or patch wry is the owner's decision.

While checking this, the review confirmed that with incognito on, WebKitGTK still puts `hsts-storage.sqlite` and its salts in the per-launch `$XDG_RUNTIME_DIR` folder, and that folder is removed on SIGTERM (exit code 0, nothing left behind). That part of `window.rs` works as described.

Resolution (main session, after the review): option (a). Incognito is off on Linux only, and stays on for macOS and Windows (`src-tauri/src/window.rs`). On Linux the whole WebKit data directory is already the per-launch folder in `$XDG_RUNTIME_DIR` that is deleted on exit, so incognito added nothing there. Re-run on the merged code: e2e 17 of 17, `.deb` and AppImage smoke tests 0 failures, nothing written to disk beyond `settings.json`.

### 10. "The app writes no other files" was not true on Windows or macOS (medium, docs fixed, follow-up 19)

`README.md:17`. The README said the app writes nothing but `settings.json`, and its own uninstall section (line 67) says `%LOCALAPPDATA%\<identifier>` is "the WebView2 runtime's working folder". WebView2 creates its user data folder even in InPrivate mode, and `scripts/smoke-macos.sh` itself looks for `~/Library/Caches/<id>` and `~/Library/WebKit/<id>` after a run. The `data_directory` redirect and cleanup in `window.rs` only exist for Linux.

Failure scenario: the owner's rule ("the only file the app writes is a settings file") is broken on two of three platforms, and the README tells users otherwise.

Fix: the README now says what the webview writes on each platform. Redirecting and deleting those folders is follow-up 19.

### 11. Linux webview folder falls back to a predictable name in the shared temp folder (low, follow-up 20)

`src-tauri/src/window.rs:59-63`, `:81`. Without `XDG_RUNTIME_DIR`, the folder is `/tmp/mr-usage-webview-<pid>`. Another local user can pre-create that name, which the app cannot delete (sticky bit), so `DirBuilder::create` fails and startup fails. The folder is also on disk, not the RAM disk the README described. The README now mentions the fallback. No data exposure was found (mode 0700, `create` rather than `create_all`).

### 12. cargo-deny did not see transitive unmaintained or unsound advisories (low, fixed)

`deny.toml:14-26`. cargo-deny's v2 default scope for `unmaintained` and `unsound` is `"workspace"`, meaning direct dependencies only. Both GTK3 advisories are transitive, so the `RUSTSEC-2024-0370` ignore matched nothing, `RUSTSEC-2024-0429` was never ignored in `deny.toml` (although `docs/FOLLOWUPS.md` item 12 said it was), and any new transitive unmaintained or unsound crate would have passed `cargo deny check`. `cargo audit -D warnings` did cover them, which is why this is low.

Fix: set `unmaintained = "all"` and `unsound = "all"`. With that change cargo-deny reported `RUSTSEC-2024-0429` as an error, so it is now ignored with a reason. Corrected FOLLOWUPS item 12.

### 13. `actions/checkout` left the job token in `.git/config` (low, fixed)

All 11 checkout steps in `.github/workflows/{ci,smoke,release}.yml`. By default `actions/checkout` persists `GITHUB_TOKEN` into the repository's git config for later steps. In `release.yml` that token has `contents: write`, and every later step runs third-party code (every crate's `build.rs`, the Tauri CLI, tauri-action).

Failure scenario: a compromised build script reads `.git/config` and pushes to the repository or edits a draft release.

Fix: `persist-credentials: false` on every checkout. No job pushes with git. Tauri-action and `gh` use the API with the token passed explicitly.

### 14. A manual release run from a branch would tag the branch name (low, fixed)

`.github/workflows/release.yml:10`, `:19`. `workflow_dispatch` is allowed and `tagName` is `${{ github.ref_name }}`. Run from `main`, tauri-action would create a tag called `main` and a draft release "Mr. Usage main".

Fix: the build job only runs when `github.ref` starts with `refs/tags/v`, and `checksums` needs it, so it is skipped too.

### 15. The e2e hint check accepted any masked value (low, fixed)

`e2e/drive.py:67`. The expected value was `••••e2f0`, but the test key ends in `e1f0`, so the first half could never match, and the fallback `startswith("••••")` also accepted the four-dot hint shown for short keys, or the wrong four characters.

Fix: the check now requires exactly `"••••" + KEY[-4:]`. It passed in the review run (`••••e1f0`).

### 16. The e2e log leak check could pass vacuously (low, fixed)

`e2e/run-linux.sh:44-50`. The script greps `app.log` for the key, `api_key` and `Authorization`. If the app's stderr ever stopped reaching `app.log` (for example a `tauri-driver` change), the grep would find nothing and pass.

Fix: the script first requires the `poll failed` warning that the fake key's first poll always logs. Confirmed present in the review run.

### 17. Manual refresh ignores Firecrawl's 30-minute floor (low, follow-up 21)

`crates/core/src/poller.rs:24`, `:381`, `crates/core/src/providers/firecrawl.rs:80`. The Refresh button is limited to once a minute for every provider. Firecrawl's floor exists because the call may cost a credit, and its card says "Checked every 30 minutes at most", which a user pressing Refresh makes untrue. Owner's call whether to enforce the floor or reword the note.

### 18. History cap is shorter than "today" at fast intervals (low, follow-up 22)

`crates/core/src/poller.rs:20`. 288 samples is a day at 5-minute polling but under five hours at the 1-minute interval SerpApi and Elicit allow. The README describes "a line of today's readings".

### 19. Changing the interval does not reschedule (low, follow-up 23)

`crates/core/src/poller.rs:258` (`apply_settings`). Existing `next_due` values are kept, so going from 1 hour to 1 minute waits up to an hour.

### 20. Copies of keys the owner's rules forbid but the libraries force (info, already follow-ups 8 and 9)

`crates/core/src/net.rs:83`. For SerpApi the key is appended into the `Url`'s internal `String` and freed without zeroizing, and reqwest keeps its own copy for the life of the request. `net.rs:77` copies the Bearer header into a `HeaderValue` (`Bytes`). `src-tauri/src/commands.rs` receives the key through serde, whose IPC-side values are freed unzeroized. The "no `String` copies of keys" rule cannot be met on these paths without replacing reqwest or Tauri's IPC. All are listed in `docs/THREAT_MODEL.md` under known residual risks.

### 21. The masked hint formats key characters (info)

`crates/core/src/keystore.rs:84`. `hint` builds `••••<last 4>` with `format!` from a `String` of the last four key characters, freed unzeroized. This is the hint the owner asked for, so no change. Noted because it is a `format!` with key material inside.

### 22. External claims without a source (low, fixed)

`crates/core/src/redact.rs:33-41`, `crates/core/src/wipe.rs:4-6`, `src-tauri/src/window.rs:4-12`, `src-tauri/src/hardening.rs:3-7`, `src-tauri/src/main.rs:36-39`, `:70-73`. Claims about key formats, `time::now_local`, WebKitGTK and WebView2 storage, crash dumps on other platforms, Tauri's origins and Windows WM_CLOSE had no URL or date. `hardening.rs` also said other platforms write no core files, which ignores Windows Error Reporting (already follow-up 10).

Fix: each now carries a `TODO(verify)` or, for the WebKitGTK file layout, the date it was observed in this review.

### 23. CI supply-chain residue (low, follow-ups 25 and 26)

`EmbarkStudios/cargo-deny-action` is pinned by SHA (verified), but its Dockerfile downloads the cargo-deny binary with `curl` and no checksum and runs an unpinned `apk update`, so the SHA does not pin what runs. The release job restores npm and Rust caches written by `main` and does not depend on the test, audit or deny jobs.

### 24. Settings file write details (low, follow-up 27)

`crates/core/src/settings.rs:76-83`. `save` writes a fixed `settings.json.tmp` (a second, short-lived file), follows a symlink planted at that name, uses default permissions and does not fsync. The file holds no secrets, so this is hygiene.

### 25. Test tooling blind spots (low, follow-ups 28 and 29)

`crates/core/tests/source_audit.rs` only checks the line that contains `expose_secret`, so a multi-line `format!(` slips through, and it ignores everything after the first test module in a file. `scripts/smoke-linux.sh` has an AppImage uninstall check that cannot fail, counts WebKit processes machine-wide, and checks only `$HOME` for stray files. `scripts/smoke-windows.ps1` ignores the NSIS installer's exit code. `e2e/run-linux.sh` does not notice a failed `tauri-driver` bind and talks to whatever owns port 4444. That happened in this review: a stale driver answered with HTTP 500.

### 26. reqwest honours proxy environment variables (info)

`crates/core/src/net.rs:52-61`. reqwest uses `HTTPS_PROXY` and `HTTP_PROXY` unless `.no_proxy()` is set. The review sandbox's proxy showed up in the e2e error text (`tunnel error: unsuccessful`). TLS stays end to end through a CONNECT tunnel and the certificate is still verified, so the proxy sees only the host name, not the key. The README's "the only network traffic is the usage call" still holds, but the proxy hop is not mentioned anywhere. Left as is: corporate users need proxies.

## Checked, no issue

- Debug on secret holders. No `Debug` derive or impl on `Auth`, `RawResponse`, `Http`, `OsKeyStore` or anything holding `SecretString`/`Zeroizing`. `KeystoreError`, `ProviderError`, `LastError` and the view types derive `Debug` but hold only scrubbed text and numbers. `source_audit` enforces the derive rule.
- Print macros and panics. No `println!`, `eprintln!` or `dbg!` in `crates/core/src` or `src-tauri/src` (the leak-test example uses `eprintln!` with `#![allow]`, and prints only a marker and errors). `unwrap`, `expect` and `panic!` are denied by Clippy and appear only in tests. The panic hook scrubs the payload.
- `expose_secret` call sites. Five in shipped code: `net.rs:73` (into a `Zeroizing` header string), `net.rs:83` (finding 20), `keystore.rs:47` (trim, then an exact-capacity copy), `keystore.rs:78` (hint, finding 21), `keystore.rs:98` (handed to keyring). None reaches a log, an error type or the frontend.
- Keys and the frontend. No command returns a key. `save_key` takes one, `key_hint` returns at most the last four characters for keys of 12 or more, `ProviderView` and `SettingsView` hold no key. `views_never_contain_the_key` and the e2e DOM check cover it.
- Commands and capability. Eight registered commands, eight `AppManifest` entries, eight grants, eight call sites in `src/ipc.ts`. No HTTP, shell, fs, opener or other plugin. `withGlobalTauri` is false. Tauri's `devtools` feature is off, so release builds have no devtools.
- CSP. No inline script or style in `dist/index.html`. Vite's module-preload polyfill is off. `connect-src 'none'` does not break IPC because Tauri's `ipc-protocol.js` falls back to `postMessage` after the blocked `fetch` (confirmed by the e2e run). No `innerHTML`, `insertAdjacentHTML`, `eval` or inline `style` in `src/`. All vendor text goes through `textContent`.
- Navigation and new windows. The `nav-guard` plugin applies to every webview and allows only the app origins. With no `on_new_window` handler, wry denies new-window requests on WebView2 (`SetHandled(true)`), and WebKitGTK and WKWebView create nothing.
- Allowlist. `check_allowed` requires `https`, the exact host, the default port and no userinfo. It runs before the request is built and again after the SerpApi query is appended. Endpoints are `&'static str`. Redirects are off, `https_only` is on, TLS 1.2 is the minimum, the timeout is 15 s, bodies are capped at 64 KiB before and during the read, and TLS verification uses `rustls-platform-verifier`.
- SerpApi response. `Body` has no `api_key`, `account_email` or `account_id`. serde skips unknown fields without allocating, and the raw body is `Zeroizing`. Transport errors use `without_url()` and are then scrubbed.
- Logging. One global subscriber, `with_ansi(false)` (ANSI field styling would hide marker words from the scrubber, and the `ansi` feature is not compiled in anyway), default level `warn`. Dependencies that use the `log` crate are dropped because `tracing-log` is not enabled. `tracing` events from hyper-util and zbus pass through the redaction writer.
- Midnight wipe. A local-date comparison on every 15-second tick covers sleep across midnight, DST and time-zone changes. Exit wipes through `RunEvent::Exit`, and SIGTERM and SIGINT route there (observed exit code 0).
- Settings. Unknown fields rejected, interval clamped, ids filtered and deduplicated, file size capped at 16 KiB. Serialized settings hold no key, email or number besides the interval.
- Disk and clipboard. No SQLite, cache or history file in the app's code. Nothing in `src/` or the Rust code touches the clipboard.
- Supply chain. All eight action SHAs match their tags (`git ls-remote`, 2026-10-16). Lockfiles are committed, npm packages all resolve from registry.npmjs.org with integrity hashes, install scripts are off, and cargo sources are restricted to crates.io. `cargo deny check`, `cargo audit -D warnings` and `npm audit --audit-level=low` all pass.
- Workflow injection. No `${{ }}` expression with attacker-controlled input appears in a `run:` block. The release tag reaches `run:` only through `env`.
- Linux hardening. `RLIMIT_CORE=0` plus `PR_SET_DUMPABLE=0` before any thread starts. On systems with `suid_dumpable=2` and a piped `core_pattern`, the kernel still hands non-dumpable processes to the helper, but systemd-coredump and apport both honour the zero core limit.

## Checks after the fixes

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | pass (67 tests, 2 ignored that need a real keystore) |
| `npm run build` | pass |
| `cargo deny check` | pass (with the stricter advisory scope) |
| `cargo audit -D warnings` | pass |
| `npm audit --audit-level=low` | 0 vulnerabilities |
| `e2e/run-linux.sh` (alternate ports) | fails at session creation with the committed `incognito(true)`. 17/17 with it removed locally (finding 9) |
