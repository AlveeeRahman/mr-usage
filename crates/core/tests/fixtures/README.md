# Test fixtures

Every file here is a response body copied verbatim from the vendor's own documentation, on 2026-10-16 for the first nine providers and 2026-10-17 for the rest. Where a vendor publishes no example, the file is the smallest body that matches the documented schema, and the table says so. If a vendor changes its documented example, replace the file and update `docs/SOURCES.md`.

| File | Source |
| --- | --- |
| `firecrawl_200.json` | https://docs.firecrawl.dev/api-reference/endpoint/credit-usage (200 example) |
| `firecrawl_404.json` | same page (404 example) |
| `firecrawl_500.json` | same page (500 example) |
| `serpapi_200.json` | https://serpapi.com/account-api ("JSON Example") |
| `serpapi_401.json` | https://serpapi.com/api-status-and-error-codes ("Error for invalid API key") |
| `serpapi_429.json` | same page ("Error for no searches remaining") |
| `elicit_200.json` | https://docs.elicit.com/ "Get usage status" response example. The site generates this example from the OpenAPI schema, so `periodStart` and `periodEnd` hold the placeholder `"string"`. The parser must cope with that, and the test checks it does. |
| `elicit_401.json` | https://docs.elicit.com/openapi.json, `GET /usage` 401 example |
| `elicit_403.json` | same, 403 example |
| `elicit_409_not_available.json` | same, 409 example `notAvailable` |
| `elicit_409_not_ready.json` | same, 409 example `notReady` |
| `elicit_429.json` | same, 429 example (flat Cloudflare body) |
| `elicit_500.json` | same, 500 example |
| `context7_200_pro.json` | https://context7.com/docs/api-reference/usage/get-api-usage (200 example "Pro plan, one member past the included requests") |
| `context7_200_free.json` | same page (200 example "Free plan") |
| `context7_401.json`, `context7_429.json`, `context7_500.json` | same page (401, 429 and 500 examples) |
| `tavily_200.json` | https://docs.tavily.com/documentation/api-reference/endpoint/usage (200 example) |
| `tavily_401.json`, `tavily_429.json` | same page (401 and 429 examples) |
| `openrouter_200.json` | https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key (200 example). Contains the echoed key label `sk-or-v1-au7...890`, a `creator_user_id` and a `workspace_id`. A test asserts none survive parsing. |
| `openrouter_401.json`, `openrouter_500.json` | same page (401 and 500 examples, where `error.code` is a number) |
| `apify_200.json` | https://docs.apify.com/api/v2/users-me-limits-get (200 example) |
| `apify_401.json`, `apify_403.json`, `apify_429.json` | same page (401, 403 and 429 examples) |
| `hunter_200_as_documented.json` | https://hunter.io/api-documentation/v2#account (200 example), exactly as published. It is not valid JSON: `"reset_date": 2026-10-24` is unquoted. A test asserts the parser rejects it. |
| `hunter_200.json` | The same example with the date quoted and nothing else changed. Contains `first_name`, `last_name` and `email`. A test asserts none survive parsing. |
| `hunter_400.json` | https://hunter.io/api-documentation/v2#errors ("Error example") |
| `runway_200.json` | https://docs.dev.runwayml.com/api/ ("Get organization information", 200 example). Generated from the schema, so every number is the placeholder 9007199254740991. |
| `brave_headers.json` | https://api-dashboard.search.brave.com/documentation/guides/rate-limiting ("Example Response Headers"), as a name/value object with lower-cased header names because reqwest lower-cases them; values verbatim. Not a body: Brave's quota lives in headers. |
| `brave_422.json` | https://api-dashboard.search.brave.com/api-reference/web/search/get shows the 422 `ErrorResponse` schema but no example body, so this is the smallest body matching it; `error.id` is made up. A test asserts it does not survive parsing. |
| `searchapi_200.json` | https://www.searchapi.io/docs/account-api ("Account Usage" response example) |
| `scrapingbee_200.json` | https://www.scrapingbee.com/documentation/#usage-endpoint ("Results." example). The renewal date has no timezone; the parser reads it as UTC and a test checks it. |
| `brightdata_200.json` | https://docs.brightdata.com/api-reference/account-management-api/Get_total_balance_through_API (200 example) |
| `linkup_200.json` | https://docs.linkup.so/pages/documentation/endpoints/account/balance (200 example) |
| `linkup_400.json` | https://docs.linkup.so/pages/documentation/platform/errors ("Here is an example") |
| `crawlbase_200.json` | https://crawlbase.com/docs/account-api lists the response fields but shows no example response, so this is the smallest body matching them. `domainStats` holds a made-up `example.com`; a test asserts it does not survive parsing. |
| `deepseek_200.json` | https://api-docs.deepseek.com/api/get-user-balance (200 example, CNY account). The currency string is parsed to an enum and a test asserts it does not survive. |
| `moonshot_200.json` | https://platform.kimi.ai/docs/api/balance ("Response Example"). `moonshot_401.json` is the page's 401/500 placeholder body, whose values are the literal string `<string>`. |
| `vercelgateway_200.json`, `vercelgateway_400.json` | https://vercel.com/docs/ai-gateway/sdks-and-apis/rest-api ("Check credit balance" sample response; the page's generic "Error response" example) |
| `novita_200.json` | https://docs.novita.ai/api-reference/basic-get-user-balance (200 example; amounts in 1/10000 USD) |
| `fal_200.json`, `fal_400.json`, `fal_401.json`, `fal_403.json`, `fal_429.json`, `fal_500.json` | https://fal.ai/docs/platform-apis/v1/account/billing (200, 400, 401, 403, 429 and 500 examples). The 200 example echoes `username: "my-team"`; a test asserts it does not survive. |
| `siliconflow_200.json`, `siliconflow_401.json` | https://docs.siliconflow.com/en/api-reference/userinfo/get-user-info (200 example, which holds placeholder `id`, `name`, `image` and `email`; a test asserts none survive. The error body is the bare string `"<string>"`.) |
| `xai_200.json` | https://docs.x.ai/developers/rest-api-reference/management/billing ("List prepaid credit balance" 200 example). Contains a `teamId`, an `invoiceId` and an `invoiceNumber`; a test asserts none survive. |
| `elevenlabs_200.json` | https://elevenlabs.io/docs/api-reference/user/subscription/get (200 example, from the page's `.md` rendering). `elevenlabs_422.json` is the smallest body matching the documented 422 schema `{detail: [{loc, msg, type}]}`; the page shows no example for it. |
| `deepgram_200.json` | https://developers.deepgram.com/reference/manage/billing/list (200 example, from the page's `.md` rendering). Contains a `balance_id` and a `purchase_order_id`; a test asserts neither survives. `deepgram_400.json` is the smallest body matching the documented legacy error schema `{err_code, err_msg, request_id}`; the page shows no example. |
| `did_200.json`, `did_404.json` | https://docs.d-id.com/reference/getcredits (OpenAPI "Example 1" for 200 and 404, from the page's `.md` rendering). The 200 example contains an `owner_id`; a test asserts it does not survive. |
| `heygen_200.json` | https://developers.heygen.com/user-profile (the "Response" example, a wallet account). Contains `username`, `email`, `first_name` and `last_name`; a test asserts none survive. The subscription and usage-based shapes have no example on the page; tests build them from the field table. |
| `stability_200.json`, `stability_401.json` | https://stabilityai.apidog.io/api-6070374 (200 "Response Example") and https://stabilityai.apidog.io/schema-1577756 (401 example), the Apidog mirror of Stability's v1 reference. The 401 body carries a request `id`; a test asserts it is not shown. |
| `bfl_200.json` | https://docs.bfl.ml/api-reference/get-the-users-credits (200 example) |
| `luma_200.json`, `luma_error.json` | https://docs.lumalabs.ai/reference/getcredits (OpenAPI definition in the page's `.md` rendering). The page shows no example 200 body, so `luma_200.json` is the smallest body matching the `Credits` schema; `luma_error.json` is the `Error` schema's own example. |
| `recraft_200.json` | https://www.recraft.ai/docs/api-reference/endpoints ("Get user information" response). Contains `email`, `id` and `name`; a test asserts none survive. |
| `runpod_200.json` | https://graphql-spec.runpod.io/ (`myself` example response, cut down to the scalar fields around `clientBalance`, `currentSpendPerHr` and `spendLimit`; every string is the placeholder `abc123`). A test asserts `id`, `authId`, `email`, `stripeSavedPaymentLast4` and `stripeAccountId` do not survive. `runpod_200_unauthorized.json` is not from the docs: the spec shows no error body, so it is the smallest standard GraphQL error envelope. |
| `vast_200.json`, `vast_401.json`, `vast_500.json` | https://docs.vast.ai/api-reference/accounts/show-user (200, 401 and 500 examples). Schema-generated: strings are `"<string>"`, numbers `123`. The 200 body carries `id`, `key_id`, `email`, `ssh_key` and `sid`; a test fills them with realistic values and asserts none survive parsing. |
| `digitalocean_200.json`, `digitalocean_401.json`, `digitalocean_404.json`, `digitalocean_429.json`, `digitalocean_500.json` | https://docs.digitalocean.com/reference/api/digitalocean/#tag/Billing/operation/balance_get (200, 401, 404, 429 and 500 examples). The 200 body has no identifiers; a test adds an `email` and a `request_id` and asserts neither survives. |
| `azurequantum_200.json` | https://learn.microsoft.com/en-us/rest/api/azurequantum/dataplane/quotas/list?view=rest-azurequantum-dataplane-2026-01-15-preview ("Get workspace quotas" sample response). `azurequantum_401.json` is not from the docs: the page shows no example error body, so it is the smallest body matching `Azure.Core.Foundations.ErrorResponse`. |
| `ahrefs_200.json`, `ahrefs_401.json`, `ahrefs_429.json`, `ahrefs_500.json` | https://docs.ahrefs.com/en/api/reference/subscription-info/get-limits-and-usage documents the schema but shows no example body, so these are the smallest bodies matching the documented schema (built 2026-10-17, not vendor copies). `ahrefs_200.json` carries the plan name `Enterprise`; a test asserts it does not survive parsing. |
| `moz_200.json` | https://moz.com/api/docs/methods/QuotaLookupAction (Response example, the bare `quota` object). Contains `account_id` 123456789; a test asserts it does not survive parsing. |
| `semrush_200.json` | https://developer.semrush.com/api/v3/get-started/api-units-balance/ ("Response example", Standard API). Plain text `1,000`, not JSON; kept under the `.json` name for the fixture glob. |
| `seranking_200.json` | https://seranking.com/api/data/account-system/ ("Get subscription details", Response example). |
| `serpstat_200.json` | https://api-docs.serpstat.com/docs/serpstat-public-api/p2grcut8yurv0-get-stats (Response Example). Contains `user_id`, `ga_client_Id` and `plan_id`; a test asserts none survive parsing. |
| `mangools_200.json`, `mangools_429.json` | https://apidocs.mangools.com/kwfinder/get-limits/ (200 and 429 examples; the 429 is the `text/html` body the page shows as a JSON string, stored unescaped). The 200 carries plan strings `premium`, `BASIC`, `orange`; a test asserts none survive. |
| `dataforseo_200.json`, `dataforseo_200_40100.json` | https://docs.dataforseo.com/v3/appendix/user_data/ documents the fields but its example is drawn by a script and is not copyable, so the 200 is the smallest body matching the field table (built 2026-10-17); the 40100 body pairs the documented envelope with the message from https://docs.dataforseo.com/v3/appendix/errors/. The 200 carries `login` `demo@example.com` and a task UUID; a test asserts neither survives. |
| `majestic_200.json` | https://developer-support.majestic.com/api/commands/get-subscription-info.shtml ("JSON response"). Contains `ServerName`, `UniqueIndexID`, `CustomerID`, `SubscriptionID`, plan codes and titles; a test asserts none survive parsing. |
| `keywordseverywhere_200.json` | https://api.keywordseverywhere.com/docs/miscellaneous/get_credits.md ("Sample Success Response"). |

`serpapi_200.json` contains the strings `SECRET_API_KEY` and `demo@serpapi.com` exactly as SerpApi documents them. A test asserts that neither string survives parsing.
