# Test fixtures

Every file here is a response body copied verbatim from the vendor's own documentation on 2026-10-16. None are invented. If a vendor changes its documented example, replace the file and update `docs/SOURCES.md`.

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

`serpapi_200.json` contains the strings `SECRET_API_KEY` and `demo@serpapi.com` exactly as SerpApi documents them. A test asserts that neither string survives parsing.
