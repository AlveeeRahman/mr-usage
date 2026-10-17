# Sources

Every external fact the code relies on, with where it came from and when. Three states:

- Verified: read from the vendor's or maintainer's own page on the date shown, and the code matches it.
- Verified live: a maintainer ran the app with a real key and saw these fields. Empty until that happens.
- Unverified: a guess, marked `TODO(verify)` in the code and never on an enabled code path without saying so on screen.

All pages below were read on 2026-10-16. Crate versions and feature flags were read from the crates.io index (`cargo info`) and docs.rs the same day.

## Firecrawl

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.firecrawl.dev/v2/team/credit-usage`, `Authorization: Bearer <key>` | Verified | https://docs.firecrawl.dev/api-reference/endpoint/credit-usage |
| 200 body `{success, data: {remainingCredits, planCredits, billingPeriodStart, billingPeriodEnd}}` | Verified | same page. Copied verbatim to `crates/core/tests/fixtures/firecrawl_200.json` |
| 404 body `{"success": false, "error": "Could not find credit usage information"}`, 500 body `{"success": false, "error": "Internal server error while fetching credit usage"}` | Verified | same page |
| Whether this call costs a credit: it does not | Verified (source code) | https://docs.firecrawl.dev/billing says "Every API call that you make consumes credits", but its per-endpoint table does not list credit-usage. The open-source API server settles it: firecrawl/firecrawl, commit `386330419c6924cc5d733e59abca9b857241acc4` (2026-10-15), `apps/api/src`: `routes/v2.ts` mounts `/team/credit-usage` behind `authMiddleware(RateLimiterMode.Account)`. `controllers/v2/credit-usage.ts` calls only `getTeamBalance` (`services/autumn/usage.ts`, a read). `billTeam(` appears in the search, batch-scrape, extract, map, crawl, agent, research-proxy and fireclaw controllers and not in this one. `services/rate-limiter.ts` gives the Account mode 1000 requests per minute. The floor is the global one minute. Confirmed live on 2026-10-17: polled from the macOS build with a real key, `remainingCredits` stayed at 1,196 of 1,500 across polls with no other usage. |
| `remainingCredits` can go below zero on pay-as-you-go (overage allowance) | Inferred | https://docs.firecrawl.dev/billing ("requests are not cut off at exactly zero"). The parser reads it as a signed number, clamps the display at zero and shows the overage separately. |
| Live response | Verified live, 2026-10-17 | macOS build, real key: `remainingCredits` 1196, `planCredits` 1500, `billingPeriodStart` 2026-10-16, `billingPeriodEnd` 2026-11-16, all parsed and shown on the card. |

## SerpApi

| Fact | State | Source |
| --- | --- | --- |
| `GET https://serpapi.com/account.json?api_key=<key>`. The only documented way to pass the key | Verified | https://serpapi.com/account-api |
| "Account API is free of charge, and using it will not be counted toward your monthly quota" | Verified | same page |
| Response fields `account_id, api_key, account_email, account_status, plan_id, plan_name, plan_monthly_price, plan_renewal_date, searches_per_month, plan_searches_left, extra_credits, total_searches_left, this_month_usage, this_hour_searches, last_hour_searches, account_rate_limit_per_hour` | Verified | same page. Example copied verbatim to `fixtures/serpapi_200.json` |
| `plan_renewal_date` is `null` for accounts without an active monthly plan | Verified | same page, "About plan_renewal_date" |
| Errors are `{"error": "<text>"}`. 401 for an invalid key, 429 for the hourly throughput limit or no searches left, 403 for a deleted account | Verified | https://serpapi.com/api-status-and-error-codes. 401 and 429 examples copied verbatim to fixtures |
| Live response | Verified live, 2026-10-17 | macOS build, real key: 200 of 250 searches left, renewal 2026-11-15, extra credits 0, this hour 0 of 250, all parsed and shown on the card. |

## Elicit

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
| 200 body `{plan, usageResetsAt, member: {requests, includedInPlan}, teamspace: {extraRequests, extraCostUsd, spendingLimitUsd, daily: [...]}}`. `plan` is `free`, `pro`, `enterprise` or `payg`. `includedInPlan` is 0 on `payg` | Verified | same page. Both 200 examples copied verbatim to `fixtures/context7_200_pro.json` and `context7_200_free.json` |
| "Calls to this endpoint do not count as requests" | Verified | same page (description). The floor is the global one minute. |
| Errors `{error, message}`: 400, 401 (`invalid_api_key`), 429 (`rate_limit_exceeded`), 500 (`internal_error`) | Verified | same page. 401, 429 and 500 copied verbatim to fixtures |
| Keys start with `ctx7sk` | Verified | the documented 401 message: "API keys should start with 'ctx7sk' prefix" |
| Live response | Not yet verified live | |

## Tavily

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.tavily.com/usage`, `Authorization: Bearer tvly-...` | Verified | https://docs.tavily.com/documentation/api-reference/endpoint/usage |
| 200 body `{key: {usage, limit, search_usage, ...}, account: {current_plan, plan_usage, plan_limit, paygo_usage, paygo_limit, ...}}` | Verified | same page. Copied verbatim to `fixtures/tavily_200.json` |
| 401 and 429 bodies `{"detail": {"error": "<text>"}}` | Verified | same page. Copied verbatim to fixtures |
| Whether this call costs a credit | Inferred | https://docs.tavily.com/documentation/api-credits lists search, extract, map, crawl and research as the credit-consuming calls. Usage is not among them. `TODO(verify)` in `providers/tavily.rs`. |
| Live response | Not yet verified live | |

## OpenRouter

| Fact | State | Source |
| --- | --- | --- |
| `GET https://openrouter.ai/api/v1/key`, `Authorization: Bearer <key>` | Verified | https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key |
| 200 body `{data: {limit, limit_remaining, limit_reset, usage, usage_daily, usage_weekly, usage_monthly, is_free_tier, effective_budget, free_model_daily_requests, label, creator_user_id, workspace_id, ...}}`, money in US dollars, `limit` and `limit_remaining` null when the key has no limit | Verified | same page. Copied verbatim to `fixtures/openrouter_200.json` |
| 401 `{"error": {"code": 401, "message": "Missing Authentication header"}}`, 500 likewise | Verified | same page. `error.code` is a number, which `providers::error_message` now accepts |
| Keys start with `sk-or-v1-` | Verified | the documented example's `label`: `sk-or-v1-au7...890` |
| `GET /api/v1/credits` needs a management key | Verified, not used | https://openrouter.ai/docs/api/api-reference/credits/get-credits ("This endpoint requires a management key") |
| Whether `GET /key` costs anything | Unverified | The docs say nothing. It is not a model call. `TODO(verify)` in `providers/openrouter.rs`. |
| Live response | Not yet verified live | |

## Apify

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.apify.com/v2/users/me/limits` | Verified | https://docs.apify.com/api/v2/users-me-limits-get |
| `Authorization: Bearer <token>` is recommended. The `token` query parameter is documented as less secure | Verified | https://docs.apify.com/api/v2#authentication |
| 200 body `{data: {monthlyUsageCycle: {startAt, endAt}, limits: {maxMonthlyUsageUsd, ...}, current: {monthlyUsageUsd, ...}}}` | Verified | first page above. Copied verbatim to `fixtures/apify_200.json` |
| Errors `{"error": {"type", "message"}}`: 400 `invalid-input`, 401 `invalid-token`, 403 `insufficient-permissions`, 405, 429 `rate-limit-exceeded` | Verified | same page. 401, 403 and 429 copied verbatim to fixtures |
| Whether this call is billable | Unverified | The page says it returns "the same information you will see on your account's Limits page". `TODO(verify)` in `providers/apify.rs`. |
| Live response | Not yet verified live | |

## Hunter

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.hunter.io/v2/account`. "This call is free." | Verified | https://hunter.io/api-documentation/v2#account |
| The key may be passed as `api_key` query, `X-API-KEY` header or `Authorization: Bearer` | Verified | https://hunter.io/api-documentation/v2#authentication |
| 200 body `{data: {first_name, last_name, email, plan_name, plan_level, reset_date, team_id, requests: {credits?, searches, verifications}, calls}}`. `credits` only for single-pool teams, `searches` and `verifications` are then approximate views of the pool | Verified | #account. The published example is not valid JSON (`"reset_date": 2026-10-24` unquoted). `fixtures/hunter_200_as_documented.json` keeps it as published, `hunter_200.json` quotes the date |
| Errors `{"errors": [{"id", "code", "details"}]}`. 401 no valid key, 403 "You have reached the rate limit", 429 "You have reached your usage limit" | Verified | https://hunter.io/api-documentation/v2#errors. The error example copied verbatim to `fixtures/hunter_400.json` |
| Live response | Not yet verified live | |

## Runway

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.dev.runwayml.com/v1/organization`, `Authorization: Bearer <key>`, required header `X-Runway-Version: 2024-11-06` ("This field must be set to the exact value") | Verified | https://docs.dev.runwayml.com/api/ ("Organization" → "Get organization information") |
| 200 body `{tier: {maxMonthlyCreditSpend, models}, creditBalance, usage: {models}}` | Verified | same page. The example is schema-generated (every number is 9007199254740991) and is copied as published to `fixtures/runway_200.json` |
| 400 "The request was rejected", 429 "You have exceeded the rate limit for this endpoint", no example bodies | Verified | same page |
| Whether this call costs credits | Unverified | The docs do not say. `TODO(verify)` in `providers/runway.rs`. |
| Live response | Not yet verified live | |

## Install scripts

| Fact | State | Source |
| --- | --- | --- |
| Gatekeeper: an app that is not notarized shows a warning. System Settings → Privacy & Security → Open Anyway saves it as an exception | Verified | https://support.apple.com/en-us/102445 |
| `xattr -dr com.apple.quarantine <app>` removes the quarantine flag that triggers that warning | Verified | `.github/scripts/smoke-macos.sh` and the macOS install-smoke job assert the attribute is absent after `install.sh` |
| `Unblock-File` removes the `Zone.Identifier` alternate data stream that marks a download as from the internet | Verified | https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/unblock-file |
| Tauri's NSIS installer installs for the current user by default, without administrator rights, into `%LOCALAPPDATA%`. `NSIS_HOOK_PREUNINSTALL` runs before files are removed | Verified | https://v2.tauri.app/distribute/windows-installer/ |
| The NSIS installer and uninstaller accept `/S` for a silent run. `msiexec /i <file> /qn` and `/x <code> /qn` install and remove an MSI without UI | Verified by CI | `.github/scripts/smoke-windows.ps1` and `install.ps1`, exercised by the Windows install-smoke job. TODO(verify): add the NSIS and msiexec documentation URLs. |
| Release file names end in `_aarch64.dmg`, `_x64.dmg`, `_x64-setup.exe`, `_x64_en-US.msi`, `_amd64.deb`, `.x86_64.rpm` and `_amd64.AppImage`. The install scripts select by these suffixes | Linux verified locally (`target/release/bundle/{deb,rpm,appimage}`). MacOS and Windows TODO(verify) | The macOS and Windows install-smoke jobs fail if the suffix is wrong, because `install.sh --from` and `install.ps1 -From` find no file. |
| `irm <url> \| iex` runs a script that starts with a `param(...)` block | Verified by example | Scoop's installer uses the same shape: https://github.com/ScoopInstaller/Install |
| `apt-get install` treats an argument as a local file only when it starts with `/` or `./`. Otherwise it is a package name | Verified | `apt-get install target/release/bundle/deb/<file>.deb` answers "Unable to locate package". `install.sh` resolves `--from` to an absolute path |
| The `.deb` and `.rpm` recommend `gnome-keyring` | Verified | `src-tauri/tauri.conf.json` `bundle.linux.{deb,rpm}.recommends`. `dpkg -I` on the built package shows `Recommends: gnome-keyring` |

## Crates and tools

| Item | Version pinned | What was checked | Source |
| --- | --- | --- | --- |
| tauri | 2.12.4 | Latest 2.x. The index's newest is 3.0.0-alpha.5, not used. Tauri 2 is the stable line. `devtools` is an opt-in feature, so release builds have no devtools. | `cargo info tauri@2`, https://v2.tauri.app/security/csp/, https://v2.tauri.app/security/capabilities/ |
| tauri-build | 2.7.3 | `AppManifest::commands` restricts the app's command set. Each command then needs its `allow-<name>` permission granted in a capability | https://v2.tauri.app/security/capabilities/ |
| @tauri-apps/api, @tauri-apps/cli | 2.12.4, 2.12.3 | npm `latest` tags | `npm view` |
| keyring | 4.2.0, feature `v1` | `v1` gives Keychain on macOS, Credential Manager on Windows, Secret Service (zbus, `crypto-rust` encrypted session) on Linux. No keyutils or file fallback. Built on keyring-core 1.x. The maintainers say apps that want to choose their own stores should link keyring-core directly. Mr. Usage wants exactly the three platform defaults, so `v1` fits. | https://docs.rs/keyring/4.2.0/keyring/, https://docs.rs/keyring/4.2.0/src/keyring/v1.rs.html, keyring-rs `Cargo.toml` |
| keyring-core error variants | 1.1.0 | `BadEncoding(Vec<u8>)` and `BadDataFormat(Vec<u8>, _)` carry raw secret bytes, `keystore.rs` zeroizes them and never formats them | keyring-core `src/error.rs` |
| keyring leak test | upstream `examples/leak-test` | Ported to `crates/core/examples/leak_test.rs` and `.github/scripts/leak-test-linux.sh` (gcore in place of procdump), plus an automated allocator-scanning version in `crates/core/tests/leak.rs` | https://github.com/open-source-cooperative/keyring-rs/tree/main/examples/leak-test |
| reqwest | 0.13.5, `rustls-no-provider` | 0.13's default TLS is rustls on aws-lc-rs. The app uses `rustls-no-provider` and installs ring instead, so the tree carries exactly one crypto provider (deny.toml bans aws-lc-rs and aws-lc-sys to keep a second one out). aws-lc-sys 0.46.0 declares `ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0)`, which the allowlist would accept. The ban is about keeping one crypto provider, not about its license. `tls_version_min`, `https_only`, `redirect(Policy::none())` and `Error::without_url()` checked against the docs and by compiling. | https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html, `cargo info reqwest@0.13.5` |
| rustls | 0.23.46, features `ring`, `std`, `tls12` | 0.24 exists only as a dev pre-release | `cargo info rustls@0.23` |
| ring | 0.17.14 | License `Apache-2.0 AND ISC`, inside the allowlist | `cargo info ring` |
| secrecy | 0.10.3, feature `serde` | `SecretString` zeroizes on drop and has a redacting `Debug` | `cargo info secrecy` |
| jiff | 0.2.38 | Local time and the midnight wipe: reads the IANA database itself and handles DST (tested in `wipe.rs`). (In time 0.3.55 the multi-threading check guards `refresh_tz`, not the offset lookup. Jiff was chosen for its bundled IANA data and DST handling.) License `Unlicense OR MIT`, so MIT applies. | `cargo info jiff` |
| libc | 0.2.190 | Latest 0.2. The index's newest is 1.0.0-alpha.5, not used. | `cargo info libc@0.2` |
| vite, typescript | 8.3.4, 7.0.2 | npm `latest` tags | `npm view` |
| GitHub Actions | commit SHAs | `actions/checkout` v7.0.1 `3d3c42e5…`, `actions/setup-node` v7.1.0 `949feb24…`, `dtolnay/rust-toolchain` stable `b725eda5…`, `Swatinem/rust-cache` v2.9.2 `6323deb1…`, `tauri-apps/tauri-action` v1.0.0 `1deb371b…`, `EmbarkStudios/cargo-deny-action` v2.1.1 `3c634983…` | `git ls-remote` on each repository |
| tauri-action workflow shape | v1 | Matrix of macOS arm64, macOS x64, Ubuntu 22.04 and Windows. Linux needs `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf` | https://v2.tauri.app/distribute/pipelines/github/ |

## Verification log

| Date | What | Result |
| --- | --- | --- |
| 2026-10-16 | `cargo test --workspace`, clippy with `-D warnings`, `cargo deny check`, `cargo audit -D warnings`, `npm run typecheck`, `npm audit` | Pass on Linux, macOS and Windows (GitHub Actions) |
| 2026-10-16 | Keystore tests against a real GNOME Keyring daemon: round trip, allocator leak scan across keyring, zbus and the encrypted session | Pass. No secret bytes in any freed heap block. |
| 2026-10-16 | `.github/scripts/leak-test-linux.sh` (full process core dump, `strings -a`) | Pass. Marker found, secret not found. |
| 2026-10-16 | `.github/e2e/run-linux.sh`: the real Linux build under Xvfb through tauri-driver and WebKitWebDriver. List providers, save a key, key lands in Secret Service, input cleared, masked hint, first poll, light and dark theme, forget key, key removed, navigation guard | 17 of 17 checks pass. The debug-level log contains neither the test key, `api_key` nor `Authorization`. The settings file holds only interval, enabled list and theme. |
| 2026-10-16 | Disk footprint with a fresh HOME and all XDG directories redirected | WebKitGTK writes `hsts-storage.sqlite`, `WebKitCache`, `storage/salt` and `mediakeys` to its data directory even with `incognito` on, so that directory is a per-launch folder in `$XDG_RUNTIME_DIR`, deleted on exit. Nothing else is written except `settings.json`. |
| 2026-10-16 | Incognito webview on Linux | WebKitWebDriver cannot attach to an incognito (ephemeral) WebKit context, so the end-to-end test could not run. Incognito is off on Linux, where the per-launch data directory already gives a throwaway store, and on for macOS and Windows. |
| 2026-10-16 | `.github/scripts/smoke-linux.sh deb` and `appimage` | 0 failures. Installs binary, desktop entry and icons. Starts and stops by window close, SIGTERM and SIGINT with exit code 0. No WebKit helpers left. Webview folder removed. Uninstall removes every installed file. AppImage extract-and-run mode exits cleanly after `PR_SET_PDEATHSIG` was added. The embedded AppImage runtime leaves its `/tmp/appimage_extracted_<md5>` folder in that mode. |
| 2026-10-16 | `.github/scripts/smoke-macos.sh`, `.github/scripts/smoke-windows.ps1` (NSIS and MSI), Fedora RPM | Pass in GitHub Actions. The Windows build needed `.github/scripts/ensure-frontend-deps.mjs`, because `npm ci` inside `beforeBuildCommand` cannot replace `node_modules` while the Tauri CLI runs from it. |
| 2026-10-16 | `install.sh --from`, then `install.sh --uninstall` (deb and AppImage). `install.ps1 -From`, then `-Uninstall` (NSIS) | Pass locally and in GitHub Actions. `dpkg -s` shows `Recommends: gnome-keyring`. A package whose hash is not in `SHA256SUMS.txt` is refused with nothing installed. |
| 2026-10-16 | Release rehearsal (`release.yml`, manual run): build all targets, draft release, `SHA256SUMS.txt`, `install.sh --version` and `install.ps1 -Version` against the draft | Linux and macOS pass. Windows: see the latest run of the release workflow. |
| 2026-10-17 | Public download path: `curl -fsSL .../installers/install.sh \| sh` with no GitHub token, after v0.1.0 was published | Pass on Ubuntu 24.04. `releases/latest` resolves to v0.1.0, the `.deb` is fetched anonymously, checked against `SHA256SUMS.txt` and installed; `install.sh --uninstall` removes it again. |
| 2026-10-17 | Firecrawl and SerpApi polled from the macOS build with real keys | Both cards fill in. The Firecrawl credit count does not move across polls, so the usage call is free on the hosted service too. |
