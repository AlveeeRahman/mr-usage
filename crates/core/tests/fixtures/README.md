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

`serpapi_200.json` contains the strings `SECRET_API_KEY` and `demo@serpapi.com` exactly as SerpApi documents them. A test asserts that neither string survives parsing.
