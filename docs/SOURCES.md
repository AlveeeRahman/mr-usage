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

## Brave Search

| Fact | State | Source |
| --- | --- | --- |
| No usage endpoint; quota is in `X-RateLimit-Limit`, `X-RateLimit-Policy`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` on every response, each `per-second, per-month` | Verified 2026-10-17 | https://api-dashboard.search.brave.com/documentation/guides/rate-limiting |
| Monthly limit `0` means unlimited; `X-RateLimit-Reset` is seconds from now | Verified 2026-10-17 | same page |
| Only successful (non-error) responses are counted and billed, so a rejected request is free | Verified 2026-10-17 | same page ("Important" note and "Key Insights") |
| Probe: `GET https://api.search.brave.com/res/v1/web/search` with no `q` (required parameter) and the key in `X-Subscription-Token`; the 422 carries the headers | Verified: `q` required and header auth per API reference; that the 422 carries the headers follows from "Every API response includes headers" | https://api-dashboard.search.brave.com/api-reference/web/search/get |
| Error body `{type:"ErrorResponse", error:{id,status,detail,meta,code}, time}` for 404/422/429; `code` in `INTERNAL`, `QUOTA_LIMITED`, `RATE_LIMITED`, ... | Verified 2026-10-17 (schema only, no example body) | same API reference |
| Fixture `brave_headers.json` | Copied from the "Example Response Headers" block, names lower-cased | rate-limiting page |
| Fixture `brave_422.json` | Built from the documented schema (no example published); `error.id` made up | API reference |

Not yet verified: Not confirmed with a live key that the 422 rejection actually carries the X-RateLimit headers (the docs say every response does). If it does not, the card returns Unsupported with a clear message rather than guessing. Behaviour on a used-up month (429 `QUOTA_LIMITED` with monthly remaining 0) is read as a zero quota; inferred from the schema, not observed.

## SearchApi.io

| Fact | State | Source |
| --- | --- | --- |
| `GET https://www.searchapi.io/api/v1/me`, key as `Authorization: Bearer` or `?api_key=` | Verified 2026-10-17 | https://www.searchapi.io/docs/account-api |
| 200 body `{account:{current_month_usage, monthly_allowance, remaining_credits}, api_usage:{searches_this_hour, hourly_rate_limit}, subscription:{period_start, period_end}}`; `subscription` only with an active subscription | Verified 2026-10-17 | same page |
| Error bodies | Not documented on the page; generic extractor used | — |
| Whether the call is billed | Not stated; treated as free (usage read, not a search) | — |
| Fixture `searchapi_200.json` | Copied verbatim from the page's Response example | same page |

Not yet verified: No documented error body; 401/403/429 mapping tested with empty bodies. Whether the call is free is not stated by the vendor.

## ScrapingBee

| Fact | State | Source |
| --- | --- | --- |
| `GET https://app.scrapingbee.com/api/v1/usage`, `Authorization: Bearer` | Verified 2026-10-17 | https://www.scrapingbee.com/documentation/#usage-endpoint |
| 200 body `{max_api_credit, used_api_credit, max_concurrency, current_concurrency, renewal_subscription_date}`; date has no offset (`2022-04-18T10:05:58.134716`), read as UTC | Verified 2026-10-17 | same section |
| "Calls to this endpoint will not increase concurrency, but you can only call it 6 times per minute" | Verified 2026-10-17 | same section |
| Status codes (written for the scraping endpoint): 401 out of credits or invalid key, 403 account banned, 429 too many concurrent requests, 400/500 with message in body; only 200/404/410/413 scrapes are billed | Verified 2026-10-17 | same page, "Status Code" table |
| Whether the usage call costs credits | Not stated explicitly; it is a usage read and treated as free | — |
| Fixture `scrapingbee_200.json` | Copied verbatim from the "Results." example | same section |

Not yet verified: No example error body is published; status mapping tested with empty and generic bodies.

## Bright Data

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.brightdata.com/customer/balance`, `Authorization: Bearer` | Verified 2026-10-17 | https://docs.brightdata.com/api-reference/account-management-api/Get_total_balance_through_API |
| 200 body `{balance, credit, prepayment, pending_costs}`, all USD; `pending_costs` = usage expected on next month's invoice | Verified 2026-10-17 | same page |
| No endpoint exposes free-tier credits (FAQ on the page) | Verified 2026-10-17 | same page |
| Error bodies | Not documented; generic extractor used | — |
| Whether the call is billed | Not stated; Account Management read, treated as free | — |
| Fixture `brightdata_200.json` | Copied verbatim from the 200 example | same page |

Not yet verified: No documented error body or status list; mapping tested with empty bodies. Free-tier accounts show funds only; free credits are not exposed by the API (documented).

## Linkup

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.linkup.so/v1/credits/balance`, `Authorization: Bearer`; 200 body `{balance}` "number of credits remaining" | Verified 2026-10-17 | https://docs.linkup.so/pages/documentation/endpoints/account/balance |
| Credits are US dollars (API key: "USD, prepaid credit balance"; $20 sign-up credit; all prices in $) | Verified 2026-10-17 | https://docs.linkup.so/pages/documentation/platform/pricing |
| Error body `{statusCode, error:{code, message, details:[{field, message}]}}`; 401 missing/invalid key, 403 no permission, 429 out of credit or too many concurrent requests, 500, 504 | Verified 2026-10-17 | https://docs.linkup.so/pages/documentation/platform/errors |
| Whether the balance call is billed | Pricing tables list search, fetch, research, tasks, extract only; balance treated as free | pricing page |
| Fixture `linkup_200.json` | Copied verbatim from the 200 example | balance page |
| Fixture `linkup_400.json` | Copied verbatim from the errors page example | errors page |

Not yet verified: The vendor does not say in so many words that the balance call is free.

## Crawlbase

| Fact | State | Source |
| --- | --- | --- |
| `GET https://api.crawlbase.com/account?token=<key>&product=crawling-api`; key only as the `token` query parameter | Verified 2026-10-17 | https://crawlbase.com/docs/account-api |
| Response fields `totalSuccess`, `totalFailed`, `totalDue` (USD), `remainingCredits` (subscription products only), `domainStats[]` (domain, totalRequests, success, failure, successRate) | Verified 2026-10-17 | same page |
| "Rate limit: 1 request per 5 minutes"; the API is "actively supported" though listed under legacy | Verified 2026-10-17 | same page |
| Error bodies | Not documented; generic extractor used | — |
| Whether the call is billed | Not stated; usage read, treated as free | — |
| Fixture `crawlbase_200.json` | Built from the documented fields (no example response published); one made-up domain | same page |

Not yet verified: No example response or error body is published; the fixture is schema-built. Only the `crawling-api` product is polled; other products (crawler, smartproxy, ...) would need a config field.

## DeepSeek

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.deepseek.com/user/balance`, `Authorization: Bearer <key>` | verified 2026-10-17 | https://api-docs.deepseek.com/api/get-user-balance |
| 200 body `{is_available, balance_infos: [{currency: CNY\|USD, total_balance, granted_balance, topped_up_balance}]}`, amounts as decimal strings in whole currency units | verified 2026-10-17 | same page (schema and example) |
| Error statuses 400, 401 Authentication Fails, 402 Insufficient Balance, 422, 429 Rate Limit Reached, 500, 503; no body shape shown | verified 2026-10-17 | https://api-docs.deepseek.com/quick_start/error_codes |
| Call is free: not a model request, no price listed on the page | inferred from the docs; not confirmed on a live key | same pages |
| Fixture `deepseek_200.json` | copied verbatim 2026-10-17 | balance page, "Example" tab |

Not yet verified: Error body shape: not documented; the module assumes the OpenAI-compatible `{"error": {"message", ...}}` form and the 429/402 tests use a hand-built body of that form (stated in the module doc). "Free" is inferred, not confirmed live.

## Moonshot (Kimi)

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.moonshot.ai/v1/users/me/balance`, `Authorization: Bearer <key>` | verified 2026-10-17 | https://platform.kimi.ai/docs/api/balance |
| 200 body `{code: 0, data: {available_balance, voucher_balance, cash_balance}, scode, status}`; amounts are JSON numbers in USD; cash may be negative; available <= 0 blocks inference | verified 2026-10-17 | same page ("Response Fields") |
| Errors 401 and 500 with `{"error": {"message", "type", "code"}}` (schema placeholder only); keys from platform.kimi.com are independent and get 401 | verified 2026-10-17 | same page |
| Call is free: not a model request, no price listed | inferred from the docs; not confirmed on a live key | same page |
| Fixtures `moonshot_200.json`, `moonshot_401.json` | copied verbatim 2026-10-17 | same page ("Response Example"; 401 placeholder body) |

Not yet verified: The docs mention platform.kimi.com as the independent platform; the task brief mentioned platform.moonshot.cn. The note names both. The .cn host's own response shape was not checked. "Free" is inferred, not confirmed live.

## Vercel AI Gateway

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://ai-gateway.vercel.sh/v1/credits`, `Authorization: Bearer <AI Gateway API key>` (OIDC token also accepted) | verified 2026-10-17 | https://vercel.com/docs/ai-gateway/sdks-and-apis/rest-api ("Check credit balance") |
| 200 body `{"balance": "95.50", "total_used": "4.50"}`, decimal strings in USD; `total_used` is lifetime | verified 2026-10-17 | same page |
| Errors `{"error": {"message", "type"}}` with 400, 401, 403 (paid plan needed), 404, 500, 503; only `/v1/models?include_availability` documents a 429 with `Retry-After: 60` | verified 2026-10-17 | same page ("Error responses") |
| Call is free: a lookup, not inference, no price listed | inferred from the docs; not confirmed on a live key | same page |
| Fixtures `vercelgateway_200.json`, `vercelgateway_400.json` | copied verbatim 2026-10-17 | same page (sample response; generic "Error response") |

Not yet verified: "Free" is inferred, not confirmed live. `spent` holds lifetime spend; the UI should not label it as a period figure.

## Novita AI

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.novita.ai/openapi/v1/billing/balance/detail`, `Authorization: Bearer <key>` | verified 2026-10-17 | https://docs.novita.ai/api-reference/basic-get-user-balance |
| 200 body `{availableBalance, cashBalance, creditLimit, pendingCharges, outstandingInvoices}`, string integers in 1/10000 USD (`10000` = $1.00); pendingCharges currently always `0` | verified 2026-10-17 | same page |
| Error bodies and rate limits: none documented on the page | verified absent 2026-10-17 | same page |
| Call is free: a billing lookup, no price listed | inferred from the docs; not confirmed on a live key | same page |
| Fixture `novita_200.json` | copied verbatim 2026-10-17 | same page (200 example) |

Not yet verified: No error body shape is documented; errors fall back to `{"error"|"message"}` text or the HTTP status. `limit = creditLimit` as the brief asked; note the documented example has availableBalance (100) above creditLimit (20), so the card can show remaining above limit. "Free" is inferred, not confirmed live.

## fal.ai

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.fal.ai/v1/account/billing?expand=credits`, header `Authorization: Key <key>` | verified 2026-10-17 | https://fal.ai/docs/platform-apis/v1/account/billing |
| 200 body `{username, credits: {current_balance: 24.5, currency: "USD"}}`; `credits` only present with `expand=credits` | verified 2026-10-17 | same page |
| Permission `billing:usage:read`, in the BILLING, READONLY and FULL key presets | verified 2026-10-17 | same page (Authorizations and page description) |
| Errors `{"error": {"type", "message"}}`: 400 validation_error, 401/403 authorization_error, 429 rate_limited, 500 server_error | verified 2026-10-17 | same page |
| Call is free: account lookup, no price listed | inferred from the docs; not confirmed on a live key | same page |
| Fixtures `fal_200.json`, `fal_400.json`, `fal_401.json`, `fal_403.json`, `fal_429.json`, `fal_500.json` | copied verbatim 2026-10-17 | same page (one example per status) |

Not yet verified: Whether 403 is only ever a scope problem (the docs show one 403 example, "Access denied"). "Free" is inferred, not confirmed live. Category set to Media (fal is image/video inference).

## SiliconFlow

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.siliconflow.com/v1/user/info`, `Authorization: Bearer <key>` | verified 2026-10-17 | https://docs.siliconflow.com/en/api-reference/userinfo/get-user-info |
| 200 body `{code: 20000, message: "OK", status: true, data: {id, name, image, email, isAdmin, balance, status, introduction, role, chargeBalance, totalBalance}}`; amounts are decimal strings | verified 2026-10-17 | same page |
| Currency is USD on the .com host | assumed from the host (the page does not name a currency) | same page |
| Errors 400, 401, 404, 429, 503, 504 with a bare JSON string body (placeholder `"<string>"`) | verified 2026-10-17 | same page |
| Call is free: account lookup, no price listed | inferred from the docs; not confirmed on a live key | same page |
| Fixtures `siliconflow_200.json`, `siliconflow_401.json` | copied verbatim 2026-10-17 | same page (200 example with placeholder identifiers; error placeholder) |

Not yet verified: The "figures differ from the console" caveat comes from the research brief; I could not verify the issue report. The note is worded as a user report. USD is inferred from the .com host, not stated on the page. The .cn host (siliconflow.cn, CNY) is not supported. "Free" is inferred, not confirmed live.

## xAI

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://management-api.x.ai/v1/billing/teams/{team_id}/prepaid/balance` | verified 2026-10-17 | https://docs.x.ai/developers/rest-api-reference/management/billing ("List prepaid credit balance and balance changes") |
| 200 body `{changes: [...], total: {val: "<USD cents string>"}}`; the ledger is inverted (a $10 purchase is `amount.val: "-1000"`, `total.val: "-1000"`) | verified 2026-10-17 | same page (field description "Representation of USD Cents" and example) |
| Auth: a Management key (separate from the inference API key, from Console -> Settings -> Management Keys) as `Authorization: Bearer <key>`; base URL `https://management-api.x.ai` | verified 2026-10-17 | https://docs.x.ai/developers/management-api-guide |
| Error bodies and statuses: none documented on the billing page | verified absent 2026-10-17 | same page |
| Call is free: management lookup, not inference, no price listed | inferred; not confirmed on a live key | same page |
| Fixture `xai_200.json` | copied verbatim 2026-10-17 | same page (200 example) |

Not yet verified: Whether a management key needs a specific ACL to read billing: the guide does not say; a 401/403 is shown as "key rejected". The team id format is not documented beyond the UUID example; the placeholder in the config field still says `team_…` (unchanged from the stub). Error shapes undocumented; generic mapping.

## ElevenLabs

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.elevenlabs.io/v1/user/subscription`, key in header `xi-api-key` | Verified 2026-10-17 | https://elevenlabs.io/docs/api-reference/user/subscription/get |
| 200 body: `tier`, `character_count`, `character_limit`, `next_character_count_reset_unix` (integer or null, seconds), `status`, `current_overage: {amount: string, currency}`, plus invoices and voice counters | Verified 2026-10-17 | same page and its `.md` rendering (https://elevenlabs.io/docs/api-reference/user/subscription/get.md) |
| Error shapes: only 422 documented, `{detail: [{loc, msg, type}]}`; 401/403/429 have no documented body | Verified 2026-10-17 | same page |
| Call is free: the page lists no price; the call reads the subscription record and synthesises nothing | Inferred from the page, not stated | same page |
| Fixture `elevenlabs_200.json` copied verbatim from the page's example response; `elevenlabs_422.json` built from the documented schema (no example shown) | Verified 2026-10-17 | `.md` rendering of the page |

Not yet verified: The docs do not say outright that the call is free; no pricing page mentions it. 401 body shape is undocumented; mapped by status alone. Regional servers (`api.us.elevenlabs.io`, `api.eu.residency.elevenlabs.io`, ...) exist per the page; the provider only contacts `api.elevenlabs.io`.

## Deepgram

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.deepgram.com/v1/projects/{project_id}/balances`, header `Authorization: Token <key>` | Verified 2026-10-17 | https://developers.deepgram.com/reference/manage/billing/list |
| 200 body `{balances: [{balance_id, amount (double), units ("USD"), purchase_order_id}]}` | Verified 2026-10-17 | same page and its `.md` rendering |
| Error shapes: 400 only, legacy `{err_code, err_msg, request_id}` or modern `{category, message, details, request_id}` | Verified 2026-10-17 | same page |
| Call is free: the page lists no price; it reads billing records and transcribes nothing | Inferred from the page, not stated | same page |
| Fixture `deepgram_200.json` copied verbatim from the page's example; `deepgram_400.json` built from the legacy error schema (no example shown) | Verified 2026-10-17 | `.md` rendering of the page |

Not yet verified: Whether the key needs a particular scope to read balances is not on the page. 401/403/429 bodies are undocumented; mapped by status alone. The project id is accepted only when it consists of `[A-Za-z0-9-]`; the page's sample id `123456-7890-1234-5678-901234` fits.

## D-ID

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.d-id.com/credits`, security `basic` or `bearer`; sample sends `authorization: Basic <key>` | Verified 2026-10-17 | https://docs.d-id.com/reference/getcredits |
| 200 body `{credits: [{owner_id, created_at, modified_at, remaining, total, expire_at, valid_from, product_id, price_id, plan_group, product_billing_interval}], remaining, total}` | Verified 2026-10-17 | same page's `.md` rendering (OpenAPI definition) |
| Error shapes: 404 `{kind: "NotFoundError", description: "not credits found for user"}` (schema `JsonError3: {kind, description, details, internal}`) | Verified 2026-10-17 | same |
| Call is free: the page lists no price; it reads credit pools and renders nothing | Inferred from the page, not stated | same |
| Fixtures `did_200.json` and `did_404.json` copied verbatim from the OpenAPI examples | Verified 2026-10-17 | same |

Not yet verified: 401/403/429 bodies are undocumented; mapped by status alone. The example's `expire_at` is in 2022, so a real account's dates were not checked against the format.

## HeyGen

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.heygen.com/v3/users/me`, header `X-Api-Key` | Verified 2026-10-17 | https://developers.heygen.com/user-profile |
| 200 body `{data: {username, email, first_name, last_name, billing_type, wallet, subscription, usage_based}}`; `billing_type` in `wallet`, `subscription`, `usage_based`; `wallet: {currency: "usd" or "credits", remaining_balance, auto_reload: {enabled, threshold_usd, amount_usd}}`; `subscription: {plan, credits: {premium_credits: {remaining, resets_at}, add_on_credits: {remaining}}}`; `usage_based: {spending_current_usd, spending_cap_usd}` | Verified 2026-10-17 | same page |
| Error shapes: none documented | Verified 2026-10-17 | same page |
| Call is free: the page lists no price and recommends the call to "check your account status or remaining quota before making API calls" | Inferred from the page, not stated | same page |
| Fixture `heygen_200.json` copied verbatim from the page's response example (wallet account) | Verified 2026-10-17 | same page |

Not yet verified: Only the wallet shape has a documented example; subscription and usage-based parsing follows the field table. No error bodies are documented; 401/403/429 are mapped by status.

## Stability AI

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.stability.ai/v1/user/balance`, key in the `Authorization` header (Bearer) | Verified 2026-10-17 on the Apidog mirror; the official SPA did not render as text | https://stabilityai.apidog.io/api-6070374 and https://stabilityai.apidog.io/doc-455573 (official: https://platform.stability.ai/docs/api-reference) |
| 200 body `{"credits": 0.6336833840314097}` (number) | Verified 2026-10-17 | https://stabilityai.apidog.io/api-6070374 |
| Error shapes: 401 and 500 listed; body `{id, name, message}`; 429 after 150 requests per 10 seconds | Verified 2026-10-17 | https://stabilityai.apidog.io/schema-1577756, https://stabilityai.apidog.io/doc-455573 |
| Call is free: no page lists a price; it reads the balance and generates nothing | Inferred, not stated | same |
| Fixtures `stability_200.json` (balance example) and `stability_401.json` (401 example) copied verbatim from the mirror | Verified 2026-10-17 | same |

Not yet verified: The official reference page is a single-page app; everything was read from the Apidog mirror, which is marked "Modified at over 2 years ago". The mirror says "Include this key in the Authorization header" without naming the scheme; Bearer is the convention the official docs use (not verified here).

## Black Forest Labs

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.bfl.ai/v1/credits`, key in header `x-key` (required) | Verified 2026-10-17 | https://docs.bfl.ml/api-reference/get-the-users-credits |
| 200 body `{"credits": 123}`, `credits` a required number, "User's current credit balance" | Verified 2026-10-17 | same page |
| Error shapes: none documented | Verified 2026-10-17 | same page |
| Call is free: the page lists no price; it reads the balance and generates nothing | Inferred, not stated | same page |
| Fixture `bfl_200.json` copied verbatim from the page's 200 example | Verified 2026-10-17 | same page |

Not yet verified: No error bodies are documented; 401/403/429 are mapped by status. Whether `credits` can be fractional in practice is not stated; the parser copes either way.

## Luma

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.lumalabs.ai/dream-machine/v1/credits`, security `BearerAuth` (`Authorization: Bearer <key>`) | Verified 2026-10-17 | https://docs.lumalabs.ai/reference/getcredits (OpenAPI in its `.md` rendering) |
| 200 schema `Credits: {credit_balance: number (float)}`, "Available credits balance in USD cents", required | Verified 2026-10-17 | same |
| Error shape: every non-200 is the `default` response `Error: {detail: string}`, example `{"detail": "Invalid API key is provided"}` | Verified 2026-10-17 | same |
| Call is free: the page lists no price; it reads the balance and generates nothing | Inferred, not stated | same |
| Fixture `luma_200.json` built from the schema (no example 200 body on the page); `luma_error.json` is the schema's own example | Verified 2026-10-17 | same |

Not yet verified: The page carries a banner pointing new integrations to the Luma API Platform (https://platform.lumalabs.ai/) and the Luma Agents API docs (https://docs.agents.lumalabs.ai/). Whether keys from that platform work on this Dream Machine endpoint is not stated; the card's note says it is for Dream Machine keys. No 200 example exists; the fixture is schema-shaped.

## Recraft

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://external.api.recraft.ai/v1/users/me`, `Authorization: Bearer RECRAFT_API_TOKEN` | Verified 2026-10-17 | https://www.recraft.ai/docs/api-reference/endpoints ("Get user information") |
| 200 body `{credits: 1000, email, id, name}` | Verified 2026-10-17 | same page |
| Error shapes: none documented | Verified 2026-10-17 | same page |
| Call is free: the endpoint table prices every call and shows "—" for `GET /users/me` | Verified 2026-10-17 | same page ("All endpoints" table) |
| Fixture `recraft_200.json` copied verbatim from the page's response example | Verified 2026-10-17 | same page |

Not yet verified: No error bodies are documented; 401/403/429 are mapped by status.

## RunPod

| Fact | State | Source |
| --- | --- | --- |
| Endpoint: `POST https://api.runpod.io/graphql`, body `{"query":"query { myself { clientBalance currentSpendPerHr spendLimit } }"}` | Verified 2026-10-17 | https://graphql-spec.runpod.io/ ("API Endpoints", `myself` query, `User` type) |
| Auth: `Authorization: Bearer <RUNPOD_API_KEY>` header | Verified 2026-10-17 | https://graphql-spec.runpod.io/ ("Headers"). https://docs.runpod.io/sdks/graphql/configurations documents `?api_key=` instead; the header is used. |
| Response shape: `{data: {myself: {clientBalance: Float, currentSpendPerHr: Float, spendLimit: Int}}}`, dollars | Verified 2026-10-17 | https://graphql-spec.runpod.io/ (`User` fields and `myself` example) |
| Error shapes: none documented. Standard GraphQL `{"errors": [{"message"}], "data": null}` on HTTP 200 is handled; auth-flavoured messages map to Unauthorized | Unverified (no vendor example) | https://graphql-spec.runpod.io/ shows no error body |
| Free: no price stated; account read, starts no pod | Unverified (inferred) | both pages |
| GraphQL API deprecated, retires early 2027 | Verified 2026-10-17 | https://docs.runpod.io/sdks/graphql/configurations |
| Fixture `runpod_200.json`: documented `myself` example cut to the scalars around the three queried fields; `runpod_200_unauthorized.json` built, not copied | 2026-10-17 | https://graphql-spec.runpod.io/ |

Not yet verified: No error body is documented; the Unauthorized mapping keys off message text ("unauthorized", "unauthenticated", "authentication", "invalid api key", "forbidden"). A live bad-key test would confirm the real wording. Whether a rejected key answers 401 or a 200 with `errors` is not documented; both are handled. The call's cost is inferred, not stated.

## Vast.ai

| Fact | State | Source |
| --- | --- | --- |
| Endpoint: `GET https://console.vast.ai/api/v0/users/current` | Verified 2026-10-17 | https://docs.vast.ai/api-reference/accounts/show-user |
| Auth: `Authorization: Bearer <token>` ("API key must be provided in the Authorization header") | Verified 2026-10-17 | same page |
| Response shape: `{id, key_id, email, balance (number<float>, dollars), ssh_key, sid}`; only `balance` is read | Verified 2026-10-17 | same page |
| Error shapes: 401 and 500 `{"success": false, "error": "<string>", "msg": "<string>"}`; no 429 documented | Verified 2026-10-17 | same page |
| Free: no price stated; account read | Unverified (inferred) | same page |
| Fixtures `vast_200.json`, `vast_401.json`, `vast_500.json` copied verbatim (schema-generated placeholders) | 2026-10-17 | same page |

Not yet verified: No limit or renewal date exists in the response; the card is balance-only. The call's cost is inferred, not stated. Vast documents no 429 for this endpoint.

## DigitalOcean

| Fact | State | Source |
| --- | --- | --- |
| Endpoint: `GET https://api.digitalocean.com/v2/customers/my/balance` | Verified 2026-10-17 | https://docs.digitalocean.com/reference/api/digitalocean/#tag/Billing/operation/balance_get |
| Auth: `Authorization: Bearer <token>`, required scope `billing:read` | Verified 2026-10-17 | same page ("Authorizations: bearer_auth, Required scopes: billing:read") |
| Response shape: `{account_balance, generated_at (date-time), month_to_date_balance, month_to_date_usage}`, amounts as decimal strings in dollars; positive `account_balance` is owed, negative is credit | Verified 2026-10-17 | same page (schema and 200 example) |
| Error shapes: `{id, message, request_id?}` for 401 `unauthorized`, 404 `not_found`, 429 `too_many_requests`, 500 `server_error` | Verified 2026-10-17 | same page |
| Free: no price stated; rate limited at 5000/hour and 250/minute via `ratelimit-*` headers | Verified rate limits 2026-10-17; price inferred | same page |
| Period end: invoices are generated on the first of each month, so the card's period ends at the start of the next UTC month | Verified text 2026-10-17; mapping inferred | same page (Billing overview) |
| Fixtures `digitalocean_200/401/404/429/500.json` copied verbatim | 2026-10-17 | same page |

Not yet verified: The sign convention (positive = owed, negative = credit) follows the research brief and the field description "Current balance of the customer's most recent billing activity"; the docs do not spell out the sign of a credit. The period end is derived from "Invoices are generated on the first of each month", not from a field in the response. The call's cost is inferred from the absence of any pricing, not stated.

## Azure Quantum

| Fact | State | Source |
| --- | --- | --- |
| Endpoint: `GET {endpoint}/subscriptions/{sub}/resourceGroups/{rg}/providers/Microsoft.Quantum/workspaces/{ws}/quotas?api-version=2026-01-15-preview`, `{endpoint}` = `https://{region}.quantum.azure.com`; the user pastes the address up to the workspace name as the "Workspace address" setting | Verified 2026-10-17 | https://learn.microsoft.com/en-us/rest/api/azurequantum/dataplane/quotas/list?view=rest-azurequantum-dataplane-2026-01-15-preview |
| Auth: `x-ms-quantum-api-key` header ("Azure Api-Key Authentication", apiKey in header); Entra OAuth2 is the alternative and is not used | Verified 2026-10-17 | same page (Security) |
| Response shape: `{value: [{dimension, scope (Workspace/Subscription), providerId, utilization, holds, limit (floats), period (Monthly/None)}], nextLink}` | Verified 2026-10-17 | same page (PagedQuota, Quota, sample response) |
| Error shapes: "Other Status Codes" return `Azure.Core.Foundations.ErrorResponse` `{"error": {code, message, target?, details?, innererror?}}` with header `x-ms-error-code`; no example body shown | Verified schema 2026-10-17 | same page (Definitions) |
| Free: no price stated; listing quotas submits no job | Unverified (inferred) | same page |
| Card: the `*hours` row with the largest limit, in seconds; remaining = limit − utilization − holds; `Monthly` ends at the start of the next UTC month ("Based on calendar year") | Verified fields 2026-10-17; mapping is ours | same page |
| Fixture `azurequantum_200.json` copied verbatim (sample response); `azurequantum_401.json` built from the ErrorResponse schema, not copied | 2026-10-17 | same page |

Not yet verified: Whether a workspace API key is accepted on the quotas operation in practice (the page lists the scheme for the operation; a live key would confirm). The exact 401/403 body wording; the Unauthorized mapping is by status. Only the first page of `value` is read; `nextLink` is ignored. Quota lists are short, but this is an assumption. The call's cost is inferred, not stated.

## IBM Quantum

| Fact | State | Source |
| --- | --- | --- |
| Auth: "You must provide an IBM Cloud Identity and Access Management (IAM) bearer token with every call as an http header. You can generate this using your API key"; token minted by `POST https://iam.cloud.ibm.com/identity/token` (`grant_type=urn:ibm:params:oauth:grant-type:apikey&apikey=…`), expires within an hour; requests then carry `Authorization: Bearer`, `Service-CRN: <instance CRN>`, `IBM-API-Version: <YYYY-MM-DD>` | Verified 2026-10-17 | https://quantum.cloud.ibm.com/docs/en/api/qiskit-runtime-rest (Authentication) |
| No documented way to send the API key directly to quantum.cloud.ibm.com | Verified 2026-10-17 (absence) | same page |
| Decision: dropped. The app contacts one host per provider and cannot call iam.cloud.ibm.com; `fetch` returns `Unsupported` with that explanation | 2026-10-17 | — |
| Endpoint `/api/v1/instances/usage` (consumed and limit seconds) | Not verified (research only; not fetched because auth blocks it) | https://quantum.cloud.ibm.com/docs/en/api/qiskit-runtime-rest |

Not yet verified: Whether IBM offers any API-key-direct auth on the quantum host outside this page (the page says IAM bearer only). If a future change lets a provider contact a second host, or a token-exchange helper lands in `net.rs`, the usage endpoint and its response shape still need verification against the instances/usage page.

## Ahrefs

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.ahrefs.com/v3/subscription-info/limits-and-usage?output=json`, `Authorization: Bearer <key>` | Verified 2026-10-17 | https://docs.ahrefs.com/en/api/reference/subscription-info/get-limits-and-usage ; header format on https://docs.ahrefs.com/en/api/docs/api-keys-creation-and-management |
| 200 body `{limits_and_usage: {api_key_expiration_date, subscription, units_limit_api_key (int or null = unlimited), units_limit_workspace (int or null), units_usage_api_key, units_usage_workspace (int or null), usage_reset_date}}` | Verified 2026-10-17 (schema only; page shows no example) | same reference page |
| Errors 400/401/403/429/500, body `{error: string}` | Verified 2026-10-17 | same reference page; 429 also at https://docs.ahrefs.com/en/api/docs/introduction ("60 requests per minute") |
| Call is free: "Requests to this endpoint are free and do not consume any API units." | Verified 2026-10-17 | same reference page |
| Fixtures `ahrefs_200.json`, `ahrefs_401.json`, `ahrefs_429.json`, `ahrefs_500.json` | Built from the documented schema (no vendor example exists) | module doc says so |

Not yet verified: No live check; the 200 fixture is schema-built. Preference of key pair over workspace pair is the app's choice.

## Moz

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `POST https://api.moz.com/jsonrpc`, header `x-moz-token: <key>`, body `{"jsonrpc":"2.0","id":..., "method":"quota.lookup","params":{"data":{"path":"api.limits.data.rows"}}}` | Verified 2026-10-17 | https://moz.com/api/docs/methods/QuotaLookupAction |
| Response `quota: {path, account_id, allotted, used, reset, report, overage, period_start, period_reset}` (Unix seconds) | Verified 2026-10-17 (example shows bare `{quota}`; app also accepts `{result: {quota}}`) | same page |
| Error shape | Not documented on the page; JSON-RPC 2.0 `error.{code,message}` assumed, HTTP 401/403 = bad key | — |
| Call is free: "Quota Usage: Does not require any quota." | Verified 2026-10-17 | same page |
| Fixture `moz_200.json` | Verbatim copy of the page's Response example | same page |

Not yet verified: Minimum `id` length (the brief said 24+; the page does not state one). The app sends a 26-char fixed id. Whether live responses are wrapped in a JSON-RPC `result` envelope (parser accepts both). JSON-RPC error shape and HTTP status for a bad token.

## Semrush

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET www.semrush.com/users/countapiunits.html?key=<key>` | Verified 2026-10-17 (page writes `http://`; app uses `https://`) | https://developer.semrush.com/api/v3/get-started/api-units-balance/ |
| Response: CSV, one value, example `1,000` = remaining API units | Verified 2026-10-17 | same page |
| Error shape for this endpoint | Not documented; page only says other APIs return `ERROR 132` / HTTP 403 when units run out | same page |
| Call is free ("free API requests") | Verified 2026-10-17 | same page |
| Fixture `semrush_200.json` (content `1,000`, not JSON) | Verbatim copy of the Response example | same page |

Not yet verified: HTTPS on `www.semrush.com` for this path (docs show http). TODO(verify) in module. Wrong-key response text; the app treats any non-numeric body mentioning "key" as Unauthorized, else Vendor.

## SE Ranking

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.seranking.com/v1/account/subscription?output=json`, header `Authorization: Token <key>` | Verified 2026-10-17 | https://seranking.com/api/data/account-system/ |
| Response `{subscription_info: {status, start_date, expiration_date ("YYYY-MM-DD HH:MM:SS"), units_limit, units_left}}` | Verified 2026-10-17 | same page |
| Error shape | Not documented | — |
| Call is free: "Cost: 0 credits" | Verified 2026-10-17 | same page |
| Fixture `seranking_200.json` | Verbatim copy of the Response example | same page |

Not yet verified: Dates are zoneless; read as UTC. Error bodies and the status for a bad key.

## Serpstat

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `POST https://api.serpstat.com/v4/?token=<key>`, body `{"id":"1","method":"SerpstatLimitsProcedure.getStats"}` | Verified 2026-10-17 | https://api-docs.serpstat.com/docs/serpstat-public-api/p2grcut8yurv0-get-stats |
| Response `{id, result: {data: {max_lines, used_lines, left_lines, user_info{...}, plugin_limits{total, used, left,...}}, summary_info: {left_lines}}}` | Verified 2026-10-17 (example is internally inconsistent: left_lines > max_lines) | same page |
| Error shape | Not documented on the page; JSON-RPC style `error.{code,message}` assumed | https://api-docs.serpstat.com/docs/serpstat-public-api/704g22jqj5lqb-errors-and-response-codes (not read) |
| Call is free: "This method doesn't spent API credits." | Verified 2026-10-17 | same page |
| Fixture `serpstat_200.json` | Verbatim copy of the Response Example | same page |

Not yet verified: Error body shape and HTTP status for a bad token (app: 200 + `error.message` mentioning "token" = Unauthorized). The Response Codes page was not fetched.

## Mangools

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.mangools.com/v3/kwfinder/limits`, header `x-access-token: <key>` (explicitly not Bearer) | Verified 2026-10-17 | https://apidocs.mangools.com/kwfinder/get-limits/ |
| Response `resources: {<pool>: {limit, remaining, reset (s or null)}}` plus `api_quota`, `permanent_credits`, plan strings | Verified 2026-10-17 | same page |
| Bad/missing token returns HTTP 200 "with nothing remaining in any pool rather than HTTP 401" | Verified 2026-10-17 | same page |
| 429 returns a `text/html` "429 Too Many Requests" page | Verified 2026-10-17 | same page |
| Cost of the call | Not stated; page describes it as for "quota checks before sending expensive requests" | same page |
| Fixtures `mangools_200.json`, `mangools_429.json` | Verbatim copies of the 200 and 429 examples (429 stored as the HTML the page shows inside a JSON string) | same page |

Not yet verified: Whether the call is free (not stated). Whether a bad token yields empty pools or pools with zero limits (app: no pool with a positive limit = Unauthorized).

## DataForSEO

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.dataforseo.com/v3/appendix/user_data`, HTTP Basic `base64(login:password)` | Verified 2026-10-17 | https://docs.dataforseo.com/v3/appendix/user_data/ |
| Response `tasks[].result[].{login, timezone, rates, money: {total, balance, limits: {day, minute}, statistics: {day, minute}}, price}` (USD) | Verified 2026-10-17 (field table; example rendered by script, not readable) | same page |
| HTTP errors 401 Unauthorized, 402 Payment Required, 404, 500; otherwise HTTP 200 with `status_code`/`status_message` (20000 ok, 40100 unauthorized, 40200 payment required, 40202 per-minute rate limit) | Verified 2026-10-17 | https://docs.dataforseo.com/v3/appendix/errors/ |
| Call is free: "Your account will not be charged for using this API" | Verified 2026-10-17 | user_data page |
| Fixtures `dataforseo_200.json`, `dataforseo_200_40100.json` | Built from the documented field table and the documented 40100 message (no readable vendor example) | module doc says so |

Not yet verified: Live shape of `money.limits` / `money.statistics` (table says nested per-function; parser reads plain numbers only). TODO(verify) in module.

## Majestic

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET https://api.majestic.com/api/json?cmd=GetSubscriptionInfo&app_api_key=<key>` | Verified 2026-10-17 (live host from the reference guide; the command page's samples use the `developer.majestic.com` sandbox) | https://developer-support.majestic.com/api/commands/get-subscription-info.shtml ; https://developer-support.majestic.com/api/ |
| Response: `Code`, `ErrorMessage`, `FullError`, `TotalAnalysisResUnits`, `TotalRetrievalResUnits`, `TotalIndexItemInfoResUnits`, `TotalDetailedReportsPerPeriodRemaining`, `SubscriptionIsActive`, `DataTables.Subscriptions.Data[].{Starts, Expires, ...}` plus server/index/plan identifiers | Verified 2026-10-17 | command page |
| Errors: HTTP 200 with `Code` != "OK" and `ErrorMessage` | Verified 2026-10-17 (no specific codes listed) | command page ("Returned values") |
| Call is free: "this command uses no resources" | Verified 2026-10-17 | command page |
| Fixture `majestic_200.json` | Verbatim copy of the "JSON response" example | command page |

Not yet verified: The exact `Code` for an invalid key (app: code or message mentioning "key"/"auth" = Unauthorized). Choice of retrieval units as the headline is the app's.

## Keywords Everywhere

| Fact | State | Source |
| --- | --- | --- |
| Endpoint `GET {{v1.server}}/account/credits`, `Authorization: Bearer <key>`, `Accept: application/json` | Verified 2026-10-17 (host template unresolved on the page; app uses `https://api.keywordseverywhere.com/v1`) | https://api.keywordseverywhere.com/docs/miscellaneous/get_credits.md |
| Response: bare JSON array with the credit balance as its only element, sample `[95597755]` | Verified 2026-10-17 | same page |
| Statuses: 200 success, 401 "API key is missing or is invalid" (no body shown) | Verified 2026-10-17 | same page |
| Cost of the call | Not stated | same page |
| Fixture `keywordseverywhere_200.json` | Verbatim copy of the Sample Success Response | same page |

Not yet verified: The `{{v1.server}}` host (assumed `https://api.keywordseverywhere.com/v1`). Whether the call is free.

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
| 2026-10-17 | 1.0.0: 35 providers added from their vendors' documentation, each with fixtures from the documented examples (or the smallest body matching the documented schema where no example is published, marked as such), 306 tests across the workspace, clippy clean | Pass locally and in GitHub Actions. Live keys have confirmed Firecrawl and SerpApi only; the rest are verified against documentation. |
