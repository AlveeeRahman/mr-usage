# Sources

Every external fact the code relies on, with where it came from and when. Three states:

- **Verified**: read from the vendor's or maintainer's own page on the date shown, and the code matches it.
- **Verified live**: the owner ran the app with a real key and saw these fields. Empty until the owner does this.
- **Unverified**: a guess, marked `TODO(verify)` in the code and never on an enabled code path without saying so on screen.

All pages below were fetched on 2026-10-16 through Firecrawl, because the build sandbox's network policy blocks direct requests to vendor sites. Crate versions and feature flags were read from the crates.io index (`cargo info`) and docs.rs the same day.

## Firecrawl

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.firecrawl.dev/v2/team/credit-usage`, `Authorization: Bearer <key>` | Verified | https://docs.firecrawl.dev/api-reference/endpoint/credit-usage |
| 200 body `{success, data: {remainingCredits, planCredits, billingPeriodStart, billingPeriodEnd}}` | Verified | same page; copied verbatim to `crates/core/tests/fixtures/firecrawl_200.json` |
| 404 body `{"success": false, "error": "Could not find credit usage information"}`, 500 body `{"success": false, "error": "Internal server error while fetching credit usage"}` | Verified | same page |
| Whether this call costs a credit | **Unverified** | https://docs.firecrawl.dev/billing says "Every API call that you make consumes credits", but its per-endpoint cost table does not list credit-usage. Because of this the provider never polls more often than every 30 minutes, and the card says why. To settle it, compare `remainingCredits` across two polls with no other usage. `TODO(verify)` in `providers/firecrawl.rs`. |
| `remainingCredits` can go below zero on pay-as-you-go (overage allowance) | Inferred | https://docs.firecrawl.dev/billing ("requests are not cut off at exactly zero"). The parser reads it as a signed number, clamps the display at zero and shows the overage separately. |
| Live response | Not yet verified live | |

## SerpApi

| Fact | State | Source |
| --- | --- | --- |
| `GET https://serpapi.com/account.json?api_key=<key>`; the only documented way to pass the key | Verified | https://serpapi.com/account-api |
| "Account API is free of charge, and using it will not be counted toward your monthly quota" | Verified | same page |
| Response fields `account_id, api_key, account_email, account_status, plan_id, plan_name, plan_monthly_price, plan_renewal_date, searches_per_month, plan_searches_left, extra_credits, total_searches_left, this_month_usage, this_hour_searches, last_hour_searches, account_rate_limit_per_hour` | Verified | same page; example copied verbatim to `fixtures/serpapi_200.json` |
| `plan_renewal_date` is `null` for accounts without an active monthly plan | Verified | same page, "About plan_renewal_date" |
| Errors are `{"error": "<text>"}`; 401 for an invalid key, 429 for the hourly throughput limit or no searches left, 403 for a deleted account | Verified | https://serpapi.com/api-status-and-error-codes; 401 and 429 examples copied verbatim to fixtures |
| Live response | Not yet verified live | |

## Elicit

The brief said no REST usage endpoint had been verified and expected a stub. Research turned up a documented one, so the provider is implemented, as the brief says to do in that case.

| Fact | State | Source |
| --- | --- | --- |
| `GET https://elicit.com/api/v2/usage`, `Authorization: Bearer elk_live_...` | Verified | https://docs.elicit.com/ ("Usage" → "Get usage status") and https://docs.elicit.com/openapi.json (OpenAPI 3.1.0, API version 2.0.0, server `https://elicit.com/api/v2`, security scheme `BearerAuth`) |
| 200 body `GetUsageResponse`: `hasUsageRemaining` (boolean), `percentUsed` (number), `periodStart` and `periodEnd` (ISO 8601 strings), `extraUsage` (`{limitUsdCents: integer or null, spentUsdCents: integer}` or null); all five required | Verified | openapi.json `components.schemas.GetUsageResponse` and `ExtraUsage` |
| Usage is reported as a percentage, not a credit count | Verified | same schema. The app shows "99% of plan left" and says so on the card. |
| Errors: 401 `invalid_api_key`, 403 `api_access_denied`, 409 `not_credits_pricing` or `usage_unavailable`, 429 flat Cloudflare body, 500 `internal_error` | Verified | openapi.json examples for `GET /usage`; all copied verbatim to fixtures |
| Rate limit: 100 requests per minute per IP, then a 5-minute block | Verified | docs.elicit.com "Billing" section |
| The only documented 200 example uses the schema placeholder `"string"` for both dates | Verified | docs.elicit.com renders the example from the schema. The parser treats an unparseable date as "no date" instead of failing; the fixture test covers this. |
| Whether `GET /usage` is billable | **Unverified** | The docs do not say. It reads the same meter the account settings page shows. `TODO(verify)` in `providers/elicit.rs`. |
| Elicit's MCP `get_usage` tool | Not used | https://docs.elicit.com/ says MCP is not a one-to-one mirror of REST. The app uses REST only. |
| Live response | Not yet verified live | |

## Crates and tools

| Item | Version pinned | What was checked | Source |
| --- | --- | --- | --- |
| tauri | 2.12.4 | Latest 2.x. The index's newest is 3.0.0-alpha.5, deliberately not used: the brief fixes Tauri 2 as the stable line. `devtools` is an opt-in feature, so release builds have no devtools. | `cargo info tauri@2`; https://v2.tauri.app/security/csp/, https://v2.tauri.app/security/capabilities/ |
| tauri-build | 2.7.3 | `AppManifest::commands` restricts the app's command set; each command then needs its `allow-<name>` permission granted in a capability | https://v2.tauri.app/security/capabilities/ |
| @tauri-apps/api, @tauri-apps/cli | 2.12.4, 2.12.3 | npm `latest` tags | `npm view` |
| keyring | 4.2.0, feature `v1` | `v1` gives Keychain on macOS, Credential Manager on Windows, Secret Service (zbus, `crypto-rust` encrypted session) on Linux. No keyutils or file fallback. Built on keyring-core 1.x. The maintainers say apps that want to choose their own stores should link keyring-core directly; Mr. Usage wants exactly the three platform defaults, so `v1` fits. | https://docs.rs/keyring/4.2.0/keyring/, https://docs.rs/keyring/4.2.0/src/keyring/v1.rs.html, keyring-rs `Cargo.toml` |
| keyring-core error variants | 1.1.0 | `BadEncoding(Vec<u8>)` and `BadDataFormat(Vec<u8>, _)` carry raw secret bytes; `keystore.rs` zeroizes them and never formats them | keyring-core `src/error.rs` |
| keyring leak test | upstream `examples/leak-test` | Ported to `crates/core/examples/leak_test.rs` and `scripts/leak-test-linux.sh` (gcore in place of procdump), plus an automated allocator-scanning version in `crates/core/tests/leak.rs` | https://github.com/open-source-cooperative/keyring-rs/tree/main/examples/leak-test |
| reqwest | 0.13.5, `rustls-no-provider` | 0.13's default TLS is rustls on aws-lc-rs. aws-lc-sys's license includes OpenSSL terms, which are outside the allowlist, so the app uses `rustls-no-provider` and installs ring. `tls_version_min`, `https_only`, `redirect(Policy::none())` and `Error::without_url()` checked against the docs and by compiling. | https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html, `cargo info reqwest@0.13.5` |
| rustls | 0.23.46, features `ring`, `std`, `tls12` | 0.24 exists only as a dev pre-release | `cargo info rustls@0.23` |
| ring | 0.17.14 | License `Apache-2.0 AND ISC`, inside the allowlist | `cargo info ring` |
| secrecy | 0.10.3, feature `serde` | `SecretString` zeroizes on drop and has a redacting `Debug` | `cargo info secrecy` |
| jiff | 0.2.38 | Used for local time because `time`'s `now_local` refuses to run in multi-threaded processes on Linux. License `Unlicense OR MIT`, so MIT applies. | `cargo info jiff` |
| libc | 0.2.190 | Latest 0.2. The index's newest is 1.0.0-alpha.5, not used. | `cargo info libc@0.2` |
| vite, typescript | 8.3.4, 7.0.2 | npm `latest` tags | `npm view` |
| GitHub Actions | commit SHAs | `actions/checkout` v7.0.1 `3d3c42e5…`, `actions/setup-node` v7.1.0 `949feb24…`, `dtolnay/rust-toolchain` stable `b725eda5…`, `Swatinem/rust-cache` v2.9.2 `6323deb1…`, `tauri-apps/tauri-action` v1.0.0 `1deb371b…`, `EmbarkStudios/cargo-deny-action` v2.1.1 `3c634983…` | `git ls-remote` on each repository |
| tauri-action workflow shape | v1 | Matrix of macOS arm64, macOS x64, Ubuntu 22.04 and Windows; Linux needs `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf` | https://v2.tauri.app/distribute/pipelines/github/ |

## Verification log

| Date | What | Result |
| --- | --- | --- |
| 2026-10-16 | `cargo test --workspace` (Linux) | All pass |
| 2026-10-16 | Ignored keystore tests against a real GNOME Keyring (Secret Service) daemon: round trip, and the allocator leak scan across keyring, zbus and the encrypted session | Pass. No secret bytes in any freed heap block. |
| 2026-10-16 | `scripts/leak-test-linux.sh` against the same daemon (full process core dump, `strings -a`) | Pass. Marker found, secret not found. |
| 2026-10-16 | End-to-end run of the real Linux build under Xvfb, driven through tauri-driver and WebKitWebDriver: list providers, save a key, key lands in Secret Service, input cleared, masked hint, first poll, light and dark theme, forget key, key removed from the store, navigation guard | 17 of 17 checks pass. The poll fails with a connect error because the sandbox blocks vendor hosts; that is the expected path. A grep of the debug-level log for the test key, `api_key` and `Authorization` found nothing. The settings file held only interval, enabled list and theme. |
| 2026-10-16 | Review re-run of `e2e/run-linux.sh` against the committed code (incognito webview) | Fails: WebKitWebDriver answers "session not created: The remote session failed to create a new browsing context". wry 0.57 builds an ephemeral WebKit context for incognito webviews with automation disabled. With `.incognito(true)` removed locally, 17 of 17 checks pass. See docs/REVIEW.md and docs/FOLLOWUPS.md. |
