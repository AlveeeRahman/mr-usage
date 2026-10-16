# Sources

Every external fact the code relies on, with where it came from and when. Three states:

- Verified: read from the vendor's or maintainer's own page on the date shown, and the code matches it.
- Verified live: the owner ran the app with a real key and saw these fields. Empty until the owner does this.
- Unverified: a guess, marked `TODO(verify)` in the code and never on an enabled code path without saying so on screen.

All pages below were fetched on 2026-10-16 through Firecrawl, because the build sandbox's network policy blocks direct requests to vendor sites. Crate versions and feature flags were read from the crates.io index (`cargo info`) and docs.rs the same day.

## Firecrawl

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.firecrawl.dev/v2/team/credit-usage`, `Authorization: Bearer <key>` | Verified | https://docs.firecrawl.dev/api-reference/endpoint/credit-usage |
| 200 body `{success, data: {remainingCredits, planCredits, billingPeriodStart, billingPeriodEnd}}` | Verified | same page. Copied verbatim to `crates/core/tests/fixtures/firecrawl_200.json` |
| 404 body `{"success": false, "error": "Could not find credit usage information"}`, 500 body `{"success": false, "error": "Internal server error while fetching credit usage"}` | Verified | same page |
| Whether this call costs a credit: it does not | Verified (source code) | https://docs.firecrawl.dev/billing says "Every API call that you make consumes credits", but its per-endpoint table does not list credit-usage. The open-source API server settles it: firecrawl/firecrawl, commit `386330419c6924cc5d733e59abca9b857241acc4` (2026-10-15), `apps/api/src`: `routes/v2.ts` mounts `/team/credit-usage` behind `authMiddleware(RateLimiterMode.Account)`; `controllers/v2/credit-usage.ts` calls only `getTeamBalance` (`services/autumn/usage.ts`, a read); `billTeam(` appears in the search, batch-scrape, extract, map, crawl, agent, research-proxy and fireclaw controllers and not in this one; `services/rate-limiter.ts` gives the Account mode 1000 requests per minute. The earlier 30-minute floor (first release) was a guess and is gone; the floor is now the global one minute. A live check with a real key is still on the list (FOLLOWUPS item 2). |
| `remainingCredits` can go below zero on pay-as-you-go (overage allowance) | Inferred | https://docs.firecrawl.dev/billing ("requests are not cut off at exactly zero"). The parser reads it as a signed number, clamps the display at zero and shows the overage separately. |
| Live response | Not yet verified live | |

## SerpApi

| Fact | State | Source |
| --- | --- | --- |
| `GET https://serpapi.com/account.json?api_key=<key>`. The only documented way to pass the key | Verified | https://serpapi.com/account-api |
| "Account API is free of charge, and using it will not be counted toward your monthly quota" | Verified | same page |
| Response fields `account_id, api_key, account_email, account_status, plan_id, plan_name, plan_monthly_price, plan_renewal_date, searches_per_month, plan_searches_left, extra_credits, total_searches_left, this_month_usage, this_hour_searches, last_hour_searches, account_rate_limit_per_hour` | Verified | same page. Example copied verbatim to `fixtures/serpapi_200.json` |
| `plan_renewal_date` is `null` for accounts without an active monthly plan | Verified | same page, "About plan_renewal_date" |
| Errors are `{"error": "<text>"}`. 401 for an invalid key, 429 for the hourly throughput limit or no searches left, 403 for a deleted account | Verified | https://serpapi.com/api-status-and-error-codes. 401 and 429 examples copied verbatim to fixtures |
| Live response | Not yet verified live | |

## Elicit

The brief said no REST usage endpoint had been verified and expected a stub. Research turned up a documented one, so the provider is implemented, as the brief says to do in that case.

| Fact | State | Source |
| --- | --- | --- |
| `GET https://elicit.com/api/v2/usage`, `Authorization: Bearer elk_live_...` | Verified | https://docs.elicit.com/ ("Usage" → "Get usage status") and https://docs.elicit.com/openapi.json (OpenAPI 3.1.0, API version 2.0.0, server `https://elicit.com/api/v2`, security scheme `BearerAuth`) |
| 200 body `GetUsageResponse`: `hasUsageRemaining` (boolean), `percentUsed` (number), `periodStart` and `periodEnd` (ISO 8601 strings), `extraUsage` (`{limitUsdCents: integer or null, spentUsdCents: integer}` or null). All five required | Verified | openapi.json `components.schemas.GetUsageResponse` and `ExtraUsage` |
| Usage is reported as a percentage, not a credit count | Verified | same schema. The app shows "99% of plan left" and says so on the card. |
| Errors: 401 `invalid_api_key`, 403 `api_access_denied`, 409 `not_credits_pricing` or `usage_unavailable`, 429 flat Cloudflare body, 500 `internal_error` | Verified | openapi.json examples for `GET /usage`. All copied verbatim to fixtures |
| Rate limit: 100 requests per minute per IP, then a 5-minute block | Verified | docs.elicit.com "Billing" section |
| The only documented 200 example uses the schema placeholder `"string"` for both dates | Verified | docs.elicit.com renders the example from the schema. The parser treats an unparseable date as "no date" instead of failing. The fixture test covers this. |
| Whether `GET /usage` is billable | Unverified | The docs do not say. It reads the same meter the account settings page shows. `TODO(verify)` in `providers/elicit.rs`. |
| Elicit's MCP `get_usage` tool | Not used | https://docs.elicit.com/ says MCP is not a one-to-one mirror of REST. The app uses REST only. |
| Live response | Not yet verified live | |

## Context7

| Fact | State | Source |
| --- | --- | --- |
| `GET https://context7.com/api/v2/usage`, `Authorization: Bearer <key>`, optional `days` query (1 to 90, default 30) | Verified | https://context7.com/docs/api-reference/usage/get-api-usage |
| 200 body `{plan, usageResetsAt, member: {requests, includedInPlan}, teamspace: {extraRequests, extraCostUsd, spendingLimitUsd, daily: [...]}}`. `plan` is `free`, `pro`, `enterprise` or `payg`; `includedInPlan` is 0 on `payg` | Verified | same page. Both 200 examples copied verbatim to `fixtures/context7_200_pro.json` and `context7_200_free.json` |
| "Calls to this endpoint do not count as requests" | Verified | same page (description). The floor is the global one minute. |
| Errors `{error, message}`: 400, 401 (`invalid_api_key`), 429 (`rate_limit_exceeded`), 500 (`internal_error`) | Verified | same page. 401, 429 and 500 copied verbatim to fixtures |
| Keys start with `ctx7sk` | Verified | the documented 401 message: "API keys should start with 'ctx7sk' prefix" |
| Context7 plugin is in Anthropic's official Claude Code plugin directory | Verified | anthropics/claude-plugins-official, `.claude-plugin/marketplace.json` at commit `ea10b213686c90bb1984b2016937310e05336cf3` (2026-10-15), entry `context7` |
| Live response | Not yet verified live | |

## Tavily

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.tavily.com/usage`, `Authorization: Bearer tvly-...` | Verified | https://docs.tavily.com/documentation/api-reference/endpoint/usage |
| 200 body `{key: {usage, limit, search_usage, ...}, account: {current_plan, plan_usage, plan_limit, paygo_usage, paygo_limit, ...}}` | Verified | same page. Copied verbatim to `fixtures/tavily_200.json` |
| 401 and 429 bodies `{"detail": {"error": "<text>"}}` | Verified | same page. Copied verbatim to fixtures |
| Whether this call costs a credit | Inferred | https://docs.tavily.com/documentation/api-credits lists search, extract, map, crawl and research as the credit-consuming calls. Usage is not among them. `TODO(verify)` in `providers/tavily.rs`. |
| Tavily plugin is in the official plugin directory | Verified | marketplace.json as above, entry `tavily` |
| Live response | Not yet verified live | |

## OpenRouter

| Fact | State | Source |
| --- | --- | --- |
| `GET https://openrouter.ai/api/v1/key`, `Authorization: Bearer <key>` | Verified | https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key |
| 200 body `{data: {limit, limit_remaining, limit_reset, usage, usage_daily, usage_weekly, usage_monthly, is_free_tier, effective_budget, free_model_daily_requests, label, creator_user_id, workspace_id, ...}}`, money in US dollars, `limit` and `limit_remaining` null when the key has no limit | Verified | same page. Copied verbatim to `fixtures/openrouter_200.json` |
| 401 `{"error": {"code": 401, "message": "Missing Authentication header"}}`, 500 likewise | Verified | same page. `error.code` is a number, which `providers::error_message` now accepts |
| Keys start with `sk-or-v1-` | Verified | the documented example's `label`: `sk-or-v1-au7...890` |
| `GET /api/v1/credits` needs a management key | Verified, not used | https://openrouter.ai/docs/api/api-reference/credits/get-credits ("This endpoint requires a management key") |
| Whether `GET /key` costs anything | Unverified | The docs say nothing; it is not a model call. `TODO(verify)` in `providers/openrouter.rs`. |
| Live response | Not yet verified live | |

## Apify

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.apify.com/v2/users/me/limits` | Verified | https://docs.apify.com/api/v2/users-me-limits-get |
| `Authorization: Bearer <token>` is recommended; the `token` query parameter is documented as less secure | Verified | https://docs.apify.com/api/v2#authentication |
| 200 body `{data: {monthlyUsageCycle: {startAt, endAt}, limits: {maxMonthlyUsageUsd, ...}, current: {monthlyUsageUsd, ...}}}` | Verified | first page above. Copied verbatim to `fixtures/apify_200.json` |
| Errors `{"error": {"type", "message"}}`: 400 `invalid-input`, 401 `invalid-token`, 403 `insufficient-permissions`, 405, 429 `rate-limit-exceeded` | Verified | same page. 401, 403 and 429 copied verbatim to fixtures |
| Whether this call is billable | Unverified | The page says it returns "the same information you will see on your account's Limits page". `TODO(verify)` in `providers/apify.rs`. |
| Apify is not in the official plugin directory | Verified | marketplace.json as above has no `apify` entry. Added because its endpoint is documented and its MCP server is widely used with Claude. |
| Live response | Not yet verified live | |

## Hunter

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.hunter.io/v2/account`; "This call is free." | Verified | https://hunter.io/api-documentation/v2#account |
| The key may be passed as `api_key` query, `X-API-KEY` header or `Authorization: Bearer` | Verified | https://hunter.io/api-documentation/v2#authentication |
| 200 body `{data: {first_name, last_name, email, plan_name, plan_level, reset_date, team_id, requests: {credits?, searches, verifications}, calls}}`; `credits` only for single-pool teams, `searches` and `verifications` are then approximate views of the pool | Verified | #account. The published example is not valid JSON (`"reset_date": 2026-10-24` unquoted); `fixtures/hunter_200_as_documented.json` keeps it as published, `hunter_200.json` quotes the date |
| Errors `{"errors": [{"id", "code", "details"}]}`; 401 no valid key, 403 "You have reached the rate limit", 429 "You have reached your usage limit" | Verified | https://hunter.io/api-documentation/v2#errors. The error example copied verbatim to `fixtures/hunter_400.json` |
| Hunter plugin is in the official plugin directory | Verified | marketplace.json as above, entry `hunter` |
| Live response | Not yet verified live | |

## Runway

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.dev.runwayml.com/v1/organization`, `Authorization: Bearer <key>`, required header `X-Runway-Version: 2024-11-06` ("This field must be set to the exact value") | Verified | https://docs.dev.runwayml.com/api/ ("Organization" → "Get organization information") |
| 200 body `{tier: {maxMonthlyCreditSpend, models}, creditBalance, usage: {models}}` | Verified | same page. The example is schema-generated (every number is 9007199254740991) and is copied as published to `fixtures/runway_200.json` |
| 400 "The request was rejected", 429 "You have exceeded the rate limit for this endpoint", no example bodies | Verified | same page |
| Whether this call costs credits | Unverified | The docs do not say. `TODO(verify)` in `providers/runway.rs`. |
| Runway plugin is in the official plugin directory | Verified | marketplace.json as above, entry `runway-api` |
| Live response | Not yet verified live | |

## Vendors looked at and not added

Each of these is in Anthropic's plugin directory or widely used with Claude, and none has a documented usage endpoint an ordinary API key can call (2026-10-16).

| Vendor | What was found | Source |
| --- | --- | --- |
| Exa | Usage is only in the Team Management API (`GET https://admin-api.exa.ai/team-management/api-keys/{id}/usage`), which needs a service key, a key id, and the feature enabled per team by support | https://exa.ai/docs/reference/team-management/get-api-key-usage.md |
| ElevenLabs | `GET https://api.elevenlabs.io/v1/user/subscription` (header `xi-api-key`) is documented with `character_count`, `character_limit` and `next_character_count_reset_unix`, but the page has no example response to use as a fixture | https://elevenlabs.io/docs/api-reference/user/subscription/get |
| Bright Data | The CLI has `brightdata budget balance`, but the documentation index names no account-balance endpoint | https://github.com/brightdata/cli, https://docs.brightdata.com/llms.txt |
| Zyte | A Stats API exists for request cost; not read in detail | https://docs.zyte.com/zyte-api/usage/stats/index.html |
| Browser Use, You.com | No usage or balance endpoint found in their documentation through search | docs.browser-use.com, documentation.you.com |
| Anthropic, Claude API (Console) | Documented: `GET /v1/organizations/usage_report/messages` and `GET /v1/organizations/cost_report` (USD cents, daily buckets, polling once a minute supported) and a Rate Limits API that returns configured limits, not remaining quota. No endpoint returns the prepaid credit balance. Accepted credentials: an Admin API key (`sk-ant-admin01-`, admins only, "every key carries full access to all endpoints that accept Admin API keys", no selectable scopes on Console keys), or a personal or service-account key not bound to one workspace, which "has the same permissions as the linked account". The Console roles page says the Billing and Limited Developer roles "can view usage and cost data". Least privilege is therefore a dedicated service account or Billing-role member with an expiring, organization-scoped key; whether such a key is accepted by `usage_report` is inferred from those two pages, not stated, and needs a live call. Not added yet: a card would show spend, not credits left, and the owner has to choose the credential. | https://platform.claude.com/docs/en/manage-claude/usage-cost-api, https://platform.claude.com/docs/en/manage-claude/admin-api, https://platform.claude.com/docs/en/manage-claude/admin-api-keys, https://platform.claude.com/docs/en/manage-claude/rate-limits-api, https://platform.claude.com/docs/en/api/beta/organization/cost_report/retrieve, https://support.claude.com/en/articles/10186004-claude-console-roles-and-permissions |
| Anthropic, Claude Pro/Max limits (5-hour, weekly, per-model weekly) | Not documented. claude.ai's Settings → Usage page and the browser extensions that mirror it read `GET https://claude.ai/api/organizations/{org}/usage` with the browser's claude.ai session cookie; Claude Code and its status line read `api.anthropic.com/api/oauth/usage` with the Claude Code OAuth token. Both are private endpoints. The official request is anthropics/claude-code#13585, open. Not added. | https://github.com/anthropics/claude-code/issues/13585, https://github.com/lugia19/Claude-Usage-Extension (manifest: host permissions `*://claude.ai/*` and `https://api.anthropic.com/*`, permissions `cookies`, `tabs`, `storage`, `alarms`, `notifications`, `contextMenus`; GPL-3.0) |
| Higgsfield | Authentication is a key ID and secret pair (`Authorization: Key ID:SECRET`), server-side only per the docs, no scopes. The documented API is submit, status, cancel, upload and `POST /estimate/<model>` (cost of a hypothetical request, `{"credits": "1.500", "usd": "0.094"}`). No balance or usage endpoint in the docs index or the OpenAPI document; the rate-limits page says the Console dashboard is the authoritative view and that no rate-limit headers are sent. Not added. | https://docs.higgsfield.ai/docs/llms.txt, https://docs.higgsfield.ai/docs/authentication.md, https://docs.higgsfield.ai/docs/concepts/billing-and-retention.md, https://docs.higgsfield.ai/docs/concepts/rate-limits.md, https://docs.higgsfield.ai/docs/openapi.json |

## Install scripts

| Fact | State | Source |
| --- | --- | --- |
| Gatekeeper: an app that is not notarized shows a warning; System Settings → Privacy & Security → Open Anyway saves it as an exception | Verified | https://support.apple.com/en-us/102445 |
| `xattr -dr com.apple.quarantine <app>` removes the quarantine flag that triggers that warning | Verified | `.github/scripts/smoke-macos.sh` and the macOS install-smoke job assert the attribute is absent after `install.sh` |
| `Unblock-File` removes the `Zone.Identifier` alternate data stream that marks a download as from the internet | Verified | https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/unblock-file |
| Tauri's NSIS installer installs for the current user by default, without administrator rights, into `%LOCALAPPDATA%`; `NSIS_HOOK_PREUNINSTALL` runs before files are removed | Verified | https://v2.tauri.app/distribute/windows-installer/ |
| The NSIS installer and uninstaller accept `/S` for a silent run; `msiexec /i <file> /qn` and `/x <code> /qn` install and remove an MSI without UI | Verified by CI | `.github/scripts/smoke-windows.ps1` and `install.ps1`, exercised by the Windows install-smoke job. TODO(verify): add the NSIS and msiexec documentation URLs. |
| Release file names end in `_aarch64.dmg`, `_x64.dmg`, `_x64-setup.exe`, `_x64_en-US.msi`, `_amd64.deb`, `.x86_64.rpm` and `_amd64.AppImage`; the install scripts select by these suffixes | Linux verified locally (`target/release/bundle/{deb,rpm,appimage}`); macOS and Windows TODO(verify) | The macOS and Windows install-smoke jobs fail if the suffix is wrong, because `install.sh --from` and `install.ps1 -From` find no file. |
| `irm <url> \| iex` runs a script that starts with a `param(...)` block | Verified by example | Scoop's installer uses the same shape: https://github.com/ScoopInstaller/Install |
| `apt-get install` treats an argument as a local file only when it starts with `/` or `./` | Verified | the first local run of `install.sh --from target/...` failed with "Unable to locate package target/release/bundle/deb"; fixed by resolving the directory to an absolute path |
| The `.deb` and `.rpm` recommend `gnome-keyring` | Verified | `src-tauri/tauri.conf.json` `bundle.linux.{deb,rpm}.recommends`; `dpkg -I` on the built package shows `Recommends: gnome-keyring` |

## Crates and tools

| Item | Version pinned | What was checked | Source |
| --- | --- | --- | --- |
| tauri | 2.12.4 | Latest 2.x. The index's newest is 3.0.0-alpha.5, deliberately not used: the brief fixes Tauri 2 as the stable line. `devtools` is an opt-in feature, so release builds have no devtools. | `cargo info tauri@2`, https://v2.tauri.app/security/csp/, https://v2.tauri.app/security/capabilities/ |
| tauri-build | 2.7.3 | `AppManifest::commands` restricts the app's command set. Each command then needs its `allow-<name>` permission granted in a capability | https://v2.tauri.app/security/capabilities/ |
| @tauri-apps/api, @tauri-apps/cli | 2.12.4, 2.12.3 | npm `latest` tags | `npm view` |
| keyring | 4.2.0, feature `v1` | `v1` gives Keychain on macOS, Credential Manager on Windows, Secret Service (zbus, `crypto-rust` encrypted session) on Linux. No keyutils or file fallback. Built on keyring-core 1.x. The maintainers say apps that want to choose their own stores should link keyring-core directly. Mr. Usage wants exactly the three platform defaults, so `v1` fits. | https://docs.rs/keyring/4.2.0/keyring/, https://docs.rs/keyring/4.2.0/src/keyring/v1.rs.html, keyring-rs `Cargo.toml` |
| keyring-core error variants | 1.1.0 | `BadEncoding(Vec<u8>)` and `BadDataFormat(Vec<u8>, _)` carry raw secret bytes, `keystore.rs` zeroizes them and never formats them | keyring-core `src/error.rs` |
| keyring leak test | upstream `examples/leak-test` | Ported to `crates/core/examples/leak_test.rs` and `.github/scripts/leak-test-linux.sh` (gcore in place of procdump), plus an automated allocator-scanning version in `crates/core/tests/leak.rs` | https://github.com/open-source-cooperative/keyring-rs/tree/main/examples/leak-test |
| reqwest | 0.13.5, `rustls-no-provider` | 0.13's default TLS is rustls on aws-lc-rs. The app uses `rustls-no-provider` and installs ring instead, so the tree carries exactly one crypto provider (deny.toml bans aws-lc-rs and aws-lc-sys to keep a second one out). Correction found by the pre-push fact check: an earlier draft said aws-lc-sys's license includes OpenSSL terms, but aws-lc-sys 0.46.0 declares `ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0)`, which the allowlist would accept. `tls_version_min`, `https_only`, `redirect(Policy::none())` and `Error::without_url()` checked against the docs and by compiling. | https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html, `cargo info reqwest@0.13.5` |
| rustls | 0.23.46, features `ring`, `std`, `tls12` | 0.24 exists only as a dev pre-release | `cargo info rustls@0.23` |
| ring | 0.17.14 | License `Apache-2.0 AND ISC`, inside the allowlist | `cargo info ring` |
| secrecy | 0.10.3, feature `serde` | `SecretString` zeroizes on drop and has a redacting `Debug` | `cargo info secrecy` |
| jiff | 0.2.38 | Local time and the midnight wipe: reads the IANA database itself and handles DST (tested in `wipe.rs`). Correction found by the pre-push fact check: an earlier draft said `time`'s `now_local` refuses to run in multi-threaded processes. In time 0.3.55 that thread check guards `refresh_tz`, not the offset lookup. License `Unlicense OR MIT`, so MIT applies. | `cargo info jiff` |
| libc | 0.2.190 | Latest 0.2. The index's newest is 1.0.0-alpha.5, not used. | `cargo info libc@0.2` |
| vite, typescript | 8.3.4, 7.0.2 | npm `latest` tags | `npm view` |
| GitHub Actions | commit SHAs | `actions/checkout` v7.0.1 `3d3c42e5…`, `actions/setup-node` v7.1.0 `949feb24…`, `dtolnay/rust-toolchain` stable `b725eda5…`, `Swatinem/rust-cache` v2.9.2 `6323deb1…`, `tauri-apps/tauri-action` v1.0.0 `1deb371b…`, `EmbarkStudios/cargo-deny-action` v2.1.1 `3c634983…` | `git ls-remote` on each repository |
| tauri-action workflow shape | v1 | Matrix of macOS arm64, macOS x64, Ubuntu 22.04 and Windows. Linux needs `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf` | https://v2.tauri.app/distribute/pipelines/github/ |

## Verification log

| Date | What | Result |
| --- | --- | --- |
| 2026-10-16 | `cargo test --workspace` (Linux) | All pass |
| 2026-10-16 | Ignored keystore tests against a real GNOME Keyring (Secret Service) daemon: round trip, and the allocator leak scan across keyring, zbus and the encrypted session | Pass. No secret bytes in any freed heap block. |
| 2026-10-16 | `.github/scripts/leak-test-linux.sh` against the same daemon (full process core dump, `strings -a`) | Pass. Marker found, secret not found. |
| 2026-10-16 | End-to-end run of the real Linux build under Xvfb, driven through tauri-driver and WebKitWebDriver: list providers, save a key, key lands in Secret Service, input cleared, masked hint, first poll, light and dark theme, forget key, key removed from the store, navigation guard | 17 of 17 checks pass. The poll fails with a connect error because the sandbox blocks vendor hosts. That is the expected path. A grep of the debug-level log for the test key, `api_key` and `Authorization` found nothing. The settings file held only interval, enabled list and theme. |
| 2026-10-16 | Review re-run of `.github/e2e/run-linux.sh` against the committed code (incognito webview) | Fails: WebKitWebDriver answers "session not created: The remote session failed to create a new browsing context". wry 0.57 builds an ephemeral WebKit context for incognito webviews with automation disabled. With `.incognito(true)` removed locally, 17 of 17 checks pass. See docs/REVIEW.md and docs/FOLLOWUPS.md. |
| 2026-10-16 | After the merge: incognito left off on Linux only (the per-launch data directory is already a throwaway store), kept on macOS and Windows. `.github/e2e/run-linux.sh` on the committed code | 17 of 17 checks pass, the log contains the expected "poll failed" line, and the key, `api_key` and `Authorization` are absent from it. |
| 2026-10-16 | After the merge: `.github/scripts/smoke-linux.sh` on rebuilt `.deb` and AppImage, real-keystore tests, core-dump leak test | `.deb` 0 failures, AppImage 0 failures (FUSE and extract-and-run modes), nothing written to disk beyond `settings.json`. Keystore round trip, allocator leak scan and core-dump leak test pass. |
| 2026-10-16 | Disk footprint of the real app with a fresh HOME and all XDG dirs redirected | WebKitGTK wrote `hsts-storage.sqlite`, `WebKitCache`, `storage/salt` and `mediakeys` to `~/.local/share/<identifier>` even with `incognito` on. Fixed by moving the webview data directory to a per-launch folder in `$XDG_RUNTIME_DIR`, deleted on exit. Re-checked: nothing written except (when settings change) `settings.json`. |
| 2026-10-16 | `.github/scripts/smoke-linux.sh deb` on the release `.deb` | 0 failures: installs binary, desktop entry and icons. Starts and stops by window close, SIGTERM and SIGINT with exit code 0, no WebKit helpers left, webview folder removed. Uninstall removes all 5 files. |
| 2026-10-16 | `.github/scripts/smoke-linux.sh appimage` on the release AppImage | 0 failures in FUSE mode (the normal desktop mode). Extract-and-run mode: app exits cleanly in all three cases (after adding `PR_SET_PDEATHSIG`. Without it, killing the runtime orphaned the app.). The embedded AppImage type2-runtime (commit 8f39b89) leaves its `/tmp/appimage_extracted_<md5>` folder in that mode. Reported, documented in the README. |
| 2026-10-16 | Windows (NSIS, MSI), macOS (DMG) and Fedora (RPM) smoke tests | Run in CI (`.github/workflows/smoke.yml`). Results recorded below once CI has run. |
| 2026-10-16 | CI for commit 9d51496 | ci: frontend, lint, supply-chain, keystore-linux and the test matrix (macOS, Ubuntu, Windows) pass; e2e-linux fails (cause unknown: the log and artifact hosts are unreachable from the sandbox, see FOLLOWUPS item 32). install-smoke: linux, fedora-rpm and macos pass; windows fails in `beforeBuildCommand` (the `npm ci` inside it could not replace `node_modules` while the Tauri CLI ran from it; fixed by `.github/scripts/ensure-frontend-deps.mjs`, which only runs `npm ci` when the lockfile is newer). |
| 2026-10-16 | After adding six providers: `cargo test --workspace` (85 core tests), clippy, deny, audit, typecheck | All pass. |
| 2026-10-16 | `.github/e2e/run-linux.sh` with nine providers | 17 of 17. |
| 2026-10-16 | `.github/scripts/smoke-linux.sh deb` on the rebuilt `.deb` (with `Recommends: gnome-keyring`) | 0 failures. |
| 2026-10-16 | `install.sh --from target/release/bundle/deb`, then `install.sh --uninstall` | Installs (`dpkg -s` shows the Recommends), `--forget-keys` reports no Secret Service in the sandbox and the uninstall continues, package and settings folder removed. A package whose hash is not in `SHA256SUMS.txt` is refused with nothing installed. |
| 2026-10-16 | CI for commit 9a69235 | keystore-linux failed: `.github/scripts/leak-test-linux.sh` still did `cd ../` to find the repository root after the move under `.github/`; fixed. supply-chain failed: `cargo audit -D warnings` reports `inout 0.2.3` as yanked; `cargo update inout` moves to the current release. Other jobs: see the watcher log in the commit after this one. |
