#!/usr/bin/env python3
"""Write one page per provider under docs/providers/ and the sitemap.

Every fact here is also in docs/SOURCES.md and the provider module of the
same name; change those first, then this table, then run:

    python3 .github/scripts/render-provider-pages.py

The pages exist so that someone searching for "check my OpenRouter balance"
or "Firecrawl credits left" lands on a page that answers that question and
names the exact endpoint the app reads, instead of a general README.
"""

from __future__ import annotations

import html
import json
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SITE = "https://alveeerahman.github.io/mr-usage/"
REPO = "https://github.com/AlveeeRahman/mr-usage"
TODAY = date.today().isoformat()

# fmt: off
PROVIDERS = [
    dict(
        id="firecrawl", name="Firecrawl",
        head="Check your Firecrawl credits on the desktop",
        lead="Mr. Usage reads Firecrawl's credit-usage endpoint with your own API key and keeps a card with the credits left, the plan size and the end of the billing period, refreshed on its own.",
        card="Credits left out of the plan, and the end of the billing period.",
        endpoint="GET https://api.firecrawl.dev/v2/team/credit-usage", auth="Authorization: Bearer <key>",
        docs="https://docs.firecrawl.dev/api-reference/endpoint/credit-usage",
        cost="Firecrawl's open-source API server bills nothing for this call: its credit-usage controller only reads the balance, and the billing function is never called from it. A live account confirmed it: the credit count did not move across polls.",
        note="Pay-as-you-go accounts can run past zero into an overage allowance. The card clamps the figure at zero and lists the overage separately.",
    ),
    dict(
        id="serpapi", name="SerpApi",
        head="Check how many SerpApi searches you have left",
        lead="Mr. Usage reads SerpApi's Account API with your key and shows the searches left this month, the renewal date and this hour's usage against your hourly limit.",
        card="Searches left this month out of the plan, the renewal date, extra credits, and this hour's searches against the hourly rate limit.",
        endpoint="GET https://serpapi.com/account.json?api_key=<key>", auth="The key travels as a query parameter, which is the only way SerpApi documents.",
        docs="https://serpapi.com/account-api",
        cost="SerpApi documents the Account API as free of charge and not counted toward the monthly quota.",
        note="The response echoes the key and the account email. The parser never reads those fields, and a test fails the build if a struct ever names them.",
    ),
    dict(
        id="elicit", name="Elicit",
        head="Check your Elicit plan usage",
        lead="Mr. Usage reads Elicit's usage endpoint with your key and shows what share of the plan is left, whether usage remains, and extra usage in dollars.",
        card="Percentage of the plan left, whether usage remains, and extra usage in dollars where the plan has it.",
        endpoint="GET https://elicit.com/api/v2/usage", auth="Authorization: Bearer <key>",
        docs="https://docs.elicit.com/",
        cost="Elicit does not say whether the usage call is billable. It reads the same meter the account page shows.",
        note="API access needs a Pro plan or above. Below that the card says so instead of showing a number. Elicit reports a percentage, not a credit count, and the card says that too.",
    ),
    dict(
        id="context7", name="Context7",
        head="Check your Context7 requests left this month",
        lead="Mr. Usage reads Context7's usage endpoint with your key and shows the requests left out of the plan, the reset date and any extra-request cost.",
        card="Requests left this month out of the plan, the reset date, and extra requests and their cost on paid plans.",
        endpoint="GET https://context7.com/api/v2/usage?days=1", auth="Authorization: Bearer <key>",
        docs="https://context7.com/docs/api-reference/usage/get-api-usage",
        cost="Context7 documents the call as free: it does not count as a request.",
        note="On pay-as-you-go plans nothing is included, so the card shows the month's request count and cost instead of a countdown.",
    ),
    dict(
        id="tavily", name="Tavily",
        head="Check your Tavily credits",
        lead="Mr. Usage reads Tavily's usage endpoint with your key and shows the credits left out of the plan, this key's own usage, and pay-as-you-go usage where the account has it.",
        card="Credits left out of the plan, this key's own usage, and pay-as-you-go usage where the account has it.",
        endpoint="GET https://api.tavily.com/usage", auth="Authorization: Bearer <key>",
        docs="https://docs.tavily.com/documentation/api-reference/endpoint/usage",
        cost="Tavily's pricing page lists search, extract, map, crawl and research as the calls that consume credits. The usage call is not among them.",
        note="",
    ),
    dict(
        id="openrouter", name="OpenRouter",
        head="Check your OpenRouter credit balance",
        lead="Mr. Usage reads the record of your OpenRouter key and shows the dollars left on its credit limit, the workspace budget, this month's spend and today's free-model requests.",
        card="Dollars left on the key's credit limit. A key without a limit shows the workspace budget, or else the month's spend. Free-model requests used today where the key has them.",
        endpoint="GET https://openrouter.ai/api/v1/key", auth="Authorization: Bearer <key>",
        docs="https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key",
        cost="This reads the key's own record and is not a model call. OpenRouter lists no price for it.",
        note="The response carries the key's label, which holds the first and last characters of the key, plus user and workspace identifiers. None of them are read.",
    ),
    dict(
        id="apify", name="Apify",
        head="Check what is left of your Apify monthly spending limit",
        lead="Mr. Usage reads Apify's limits endpoint with your token and shows the dollars left of this month's spending limit, as the console's Limits page does, plus compute units and proxy SERPs used.",
        card="Dollars left of this month's spending limit, with compute units and proxy SERPs used against their limits, and the end of the usage cycle.",
        endpoint="GET https://api.apify.com/v2/users/me/limits", auth="Authorization: Bearer <token>",
        docs="https://docs.apify.com/api/v2/users-me-limits-get",
        cost="The call reads what the console's Limits page shows. Apify does not list it as billable platform usage.",
        note="",
    ),
    dict(
        id="hunter", name="Hunter",
        head="Check your Hunter searches and verifications left",
        lead="Mr. Usage reads Hunter's account endpoint with your key and shows the searches left, the verifications left, the credit pool on plans that have one, and the reset date.",
        card="Searches left, verifications left, the credit pool on plans that have one, and the reset date.",
        endpoint="GET https://api.hunter.io/v2/account", auth="Authorization: Bearer <key>",
        docs="https://hunter.io/api-documentation/v2#account",
        cost="Hunter documents the account call as free.",
        note="The response carries the owner's name and email. They are never read.",
    ),
    dict(
        id="runway", name="Runway",
        head="Check your Runway credit balance",
        lead="Mr. Usage reads Runway's organization endpoint with your key and shows the organization's credit balance and the monthly spend cap.",
        card="The organization's credit balance, and the monthly spend cap where the tier sets one.",
        endpoint="GET https://api.dev.runwayml.com/v1/organization", auth="Authorization: Bearer <key>, plus the required header X-Runway-Version: 2024-11-06",
        docs="https://docs.dev.runwayml.com/api/#tag/Organization/paths/~1v1~1organization/get",
        cost="Runway does not document whether this call costs credits.",
        note="",
    ),
    dict(
        id='brave', name='Brave Search',
        head='Check how much of your Brave Search plan is left',
        lead="Mr. Usage reads Brave Search's documented usage endpoint with your own key and keeps a card with searches left this month of the plan's monthly quota, and when the month resets, refreshed on its own.",
        card="Searches left this month of the plan's monthly quota, and when the month resets.",
        endpoint='`GET https://api.search.brave.com/res/v1/web/search` (no `q`).', auth='header `X-Subscription-Token`.',
        docs='https://api-dashboard.search.brave.com/documentation/guides/rate-limiting',
        cost="free; Brave counts and bills only successful responses, and the card's request is rejected on validation.",
        note='Brave has no usage endpoint. The card sends a search request the API rejects on validation and reads the quota headers on the rejection; Brave bills only successful requests.',
    ),
    dict(
        id='searchapi', name='SearchApi.io',
        head='Check how much of your SearchApi.io plan is left',
        lead="Mr. Usage reads SearchApi.io's documented usage endpoint with your own key and keeps a card with search credits left this month of the monthly allowance, the subscription period end, searches used this month and this hour, refreshed on its own.",
        card='Search credits left this month of the monthly allowance, the subscription period end, searches used this month and this hour.',
        endpoint='`GET https://www.searchapi.io/api/v1/me`', auth='header `Authorization: Bearer`.',
        docs='https://www.searchapi.io/docs/account-api',
        cost='the docs do not say; it is an account read, not a search, and is treated as free.',
        note='',
    ),
    dict(
        id='scrapingbee', name='ScrapingBee',
        head='Check how much of your ScrapingBee plan is left',
        lead="Mr. Usage reads ScrapingBee's documented usage endpoint with your own key and keeps a card with API credits left of the plan's credits, the renewal date, and current concurrency, refreshed on its own.",
        card="API credits left of the plan's credits, the renewal date, and current concurrency.",
        endpoint='`GET https://app.scrapingbee.com/api/v1/usage`', auth='header `Authorization: Bearer`.',
        docs='https://www.scrapingbee.com/documentation/#usage-endpoint',
        cost="free of credits per the docs' billing table (only scrapes are billed); the usage endpoint is limited to six calls a minute.",
        note='ScrapingBee allows six usage reads a minute; the card polls at most once a minute.',
    ),
    dict(
        id='brightdata', name='Bright Data',
        head='Check what you have left at Bright Data',
        lead="Mr. Usage reads Bright Data's documented usage endpoint with your own key and keeps a card with account funds in dollars, this period's pending costs, and any credit or prepayment, refreshed on its own.",
        card="Account funds in dollars, this period's pending costs, and any credit or prepayment.",
        endpoint='`GET https://api.brightdata.com/customer/balance`', auth='header `Authorization: Bearer`.',
        docs='https://docs.brightdata.com/api-reference/account-management-api/Get_total_balance_through_API',
        cost='the docs do not say; it is an account balance read, not a proxy or scraper request, and is treated as free.',
        note="Shows account funds. Bright Data's API does not expose free-tier credits.",
    ),
    dict(
        id='linkup', name='Linkup',
        head='Check your Linkup balance',
        lead="Mr. Usage reads Linkup's documented usage endpoint with your own key and keeps a card with the prepaid credit balance in dollars, refreshed on its own.",
        card='The prepaid credit balance in dollars.',
        endpoint='`GET https://api.linkup.so/v1/credits/balance`', auth='header `Authorization: Bearer`.',
        docs='https://docs.linkup.so/pages/documentation/endpoints/account/balance',
        cost="not in any of Linkup's pricing tables, which bill search, fetch, research, tasks and extract; treated as free.",
        note="Shows the prepaid balance in dollars. Linkup's API has no spend history.",
    ),
    dict(
        id='crawlbase', name='Crawlbase',
        head='Check what you have left at Crawlbase',
        lead="Mr. Usage reads Crawlbase's documented usage endpoint with your own key and keeps a card with crawling API credits left this month on a subscription, or this month's amount due in dollars on pay as you go, with successful and failed request counts, refreshed on its own.",
        card="Crawling API credits left this month on a subscription, or this month's amount due in dollars on pay as you go, with successful and failed request counts.",
        endpoint='`GET https://api.crawlbase.com/account?product=crawling-api`', auth='query parameter `token`.',
        docs='https://crawlbase.com/docs/account-api',
        cost='the docs do not say; it is a usage read and treated as free. Crawlbase allows one call every five minutes, so the card polls no faster (300 s floor).',
        note='Crawlbase allows one usage read every five minutes, so this card refreshes no faster. Shows the Crawling API product.',
    ),
    dict(
        id='deepseek', name='DeepSeek',
        head='Check your DeepSeek balance',
        lead="Mr. Usage reads DeepSeek's documented usage endpoint with your own key and keeps a card with the account's remaining balance in the currency DeepSeek bills you in (yuan or dollars), with the granted and topped-up parts as extras and a flag when inference is blocked, refreshed on its own.",
        card="The account's remaining balance in the currency DeepSeek bills you in (yuan or dollars), with the granted and topped-up parts as extras and a flag when inference is blocked.",
        endpoint='`GET https://api.deepseek.com/user/balance`', auth='`Authorization: Bearer <key>` header.',
        docs='https://api-docs.deepseek.com/api/get-user-balance',
        cost='the balance call is not a model request and the docs list no price; it consumes no balance.',
        note='Shows the account balance in the currency DeepSeek bills you in, yuan or dollars.',
    ),
    dict(
        id='moonshot', name='Moonshot (Kimi)',
        head='Check your Moonshot (Kimi) balance',
        lead="Mr. Usage reads Moonshot (Kimi)'s documented usage endpoint with your own key and keeps a card with the spendable balance (cash plus vouchers) in dollars, with voucher and cash balances as extras; a negative cash balance shows as debt, refreshed on its own.",
        card='The spendable balance (cash plus vouchers) in dollars, with voucher and cash balances as extras; a negative cash balance shows as debt.',
        endpoint='`GET https://api.moonshot.ai/v1/users/me/balance`', auth='`Authorization: Bearer <key>` header, key from platform.kimi.ai.',
        docs='https://platform.kimi.ai/docs/api/balance',
        cost='the balance call is not a model request and the docs list no price; it consumes no balance.',
        note='For keys from platform.kimi.ai. Keys from platform.kimi.com or platform.moonshot.cn use a different host and are not supported.',
    ),
    dict(
        id='vercelgateway', name='Vercel AI Gateway',
        head='Check what you have left at Vercel AI Gateway',
        lead='Mr. Usage reads Vercel AI Gateway\'s documented usage endpoint with your own key and keeps a card with the team\'s remaining AI Gateway credit in dollars, plus "Used so far" (lifetime spend, not this month\'s), refreshed on its own.',
        card='The team\'s remaining AI Gateway credit in dollars, plus "Used so far" (lifetime spend, not this month\'s).',
        endpoint='`GET https://ai-gateway.vercel.sh/v1/credits`', auth='`Authorization: Bearer <key>` header.',
        docs='https://vercel.com/docs/ai-gateway/sdks-and-apis/rest-api',
        cost='the credits call is a lookup, not an inference request; the docs list no price and it consumes no credits.',
        note='Shows the team\'s credit balance. "Used so far" is lifetime spend, not this month\'s.',
    ),
    dict(
        id='novita', name='Novita AI',
        head='Check your Novita AI balance',
        lead="Mr. Usage reads Novita AI's documented usage endpoint with your own key and keeps a card with the spendable balance in dollars; when the account has a credit line it is shown as the limit, with the top-up balance and any amount owed as extras, refreshed on its own.",
        card='The spendable balance in dollars; when the account has a credit line it is shown as the limit, with the top-up balance and any amount owed as extras.',
        endpoint='`GET https://api.novita.ai/openapi/v1/billing/balance/detail`', auth='`Authorization: Bearer <key>` header.',
        docs='https://docs.novita.ai/api-reference/basic-get-user-balance',
        cost='the balance call is a billing lookup, not a model request; the docs list no price and it consumes no balance.',
        note='Shows the spendable balance. The limit, when shown, is the credit line Novita lets you owe.',
    ),
    dict(
        id='fal', name='fal.ai',
        head='Check your fal.ai balance',
        lead="Mr. Usage reads fal.ai's documented usage endpoint with your own key and keeps a card with the account's remaining credit balance in dollars, refreshed on its own.",
        card="The account's remaining credit balance in dollars.",
        endpoint='`GET https://api.fal.ai/v1/account/billing?expand=credits`', auth='`Authorization: Key <key>` header; the key must be on the READONLY or BILLING preset (permission billing:usage:read). A key without it gets 403, shown as "unsupported" with that hint.',
        docs='https://fal.ai/docs/platform-apis/v1/account/billing',
        cost='the billing call is an account lookup, not a model request; the docs list no price and it consumes no credits.',
        note='Needs a key on the READONLY or BILLING preset (permission billing:usage:read).',
    ),
    dict(
        id='siliconflow', name='SiliconFlow',
        head='Check your SiliconFlow balance',
        lead="Mr. Usage reads SiliconFlow's documented usage endpoint with your own key and keeps a card with the total balance in dollars, with the granted and topped-up parts as extras, refreshed on its own.",
        card='The total balance in dollars, with the granted and topped-up parts as extras.',
        endpoint='`GET https://api.siliconflow.com/v1/user/info`', auth='`Authorization: Bearer <key>` header, key from siliconflow.com.',
        docs='https://docs.siliconflow.com/en/api-reference/userinfo/get-user-info',
        cost='the user-info call is an account lookup, not a model request; the docs list no price and it consumes no balance.',
        note='For keys from siliconflow.com (billed in dollars). Users have reported the figures differing from the console.',
    ),
    dict(
        id='xai', name='xAI',
        head='Check your xAI balance',
        lead="Mr. Usage reads xAI's documented usage endpoint with your own key and keeps a card with the team's prepaid credit balance in dollars (postpaid usage and spending limits are not shown), refreshed on its own.",
        card="The team's prepaid credit balance in dollars (postpaid usage and spending limits are not shown).",
        endpoint='`GET https://management-api.x.ai/v1/billing/teams/<team id>/prepaid/balance`', auth="a Management API key (not an inference key) as `Authorization: Bearer <key>`, plus the team id entered in the provider's settings (letters, digits, `-` and `_` only).",
        docs='https://docs.x.ai/developers/rest-api-reference/management/billing',
        cost='the management call is a billing lookup, not inference; the docs list no price and it consumes no credits.',
        note='Needs a Management API key, not an inference key, plus the team id. Shows prepaid credit only.',
    ),
    dict(
        id='elevenlabs', name='ElevenLabs',
        head='Check how much of your ElevenLabs plan is left',
        lead="Mr. Usage reads ElevenLabs's documented usage endpoint with your own key and keeps a card with characters left out of the plan's character limit for the current billing period, with the next reset date, the plan tier, and any usage-based overage spend, refreshed on its own.",
        card="Characters left out of the plan's character limit for the current billing period, with the next reset date, the plan tier, and any usage-based overage spend.",
        endpoint='`GET https://api.elevenlabs.io/v1/user/subscription`', auth='',
        docs='https://elevenlabs.io/docs/api-reference/user/subscription/get',
        cost='the docs list no price for this call; it reads the subscription and generates no speech.',
        note='Counts characters in the current billing period. Overage spend shows as an extra when usage-based billing is on.',
    ),
    dict(
        id='deepgram', name='Deepgram',
        head='Check your Deepgram balance',
        lead="Mr. Usage reads Deepgram's documented usage endpoint with your own key and keeps a card with the project's outstanding prepaid balance in US dollars, adding up every balance whose units are USD, refreshed on its own.",
        card="The project's outstanding prepaid balance in US dollars, adding up every balance whose units are USD.",
        endpoint='`GET https://api.deepgram.com/v1/projects/<project id>/balances`', auth='',
        docs='https://developers.deepgram.com/reference/manage/billing/list',
        cost='the docs list no price for this call; it reads billing records and processes no audio.',
        note="Adds up the project's outstanding US dollar balances. Needs the project id from the console.",
    ),
    dict(
        id='did', name='D-ID',
        head='Check how much of your D-ID plan is left',
        lead="Mr. Usage reads D-ID's documented usage endpoint with your own key and keeps a card with credits remaining out of the total across the account's credit pools, with the renewal date set to when the first pool that still has credits expires, refreshed on its own.",
        card="Credits remaining out of the total across the account's credit pools, with the renewal date set to when the first pool that still has credits expires.",
        endpoint='`GET https://api.d-id.com/credits`', auth='',
        docs='https://docs.d-id.com/reference/getcredits',
        cost='the docs list no price for this call; it reads credit pools and renders no video.',
        note='The renewal date is when the first credit pool that still has credits expires.',
    ),
    dict(
        id='heygen', name='HeyGen',
        head='Check your HeyGen balance',
        lead="Mr. Usage reads HeyGen's documented usage endpoint with your own key and keeps a card with whichever the account uses: the prepaid wallet balance (dollars or credits), the subscription's premium credits with their reset date, or the metered spend against its cap, refreshed on its own.",
        card="Whichever the account uses: the prepaid wallet balance (dollars or credits), the subscription's premium credits with their reset date, or the metered spend against its cap.",
        endpoint='`GET https://api.heygen.com/v3/users/me`', auth='',
        docs='https://developers.heygen.com/user-profile',
        cost='the docs list no price for this call and present it as the way to check remaining quota before generating.',
        note="Shows the wallet balance, the subscription's premium credits or the metered spend, whichever the account uses.",
    ),
    dict(
        id='stability', name='Stability AI',
        head='Check your Stability AI balance',
        lead="Mr. Usage reads Stability AI's documented usage endpoint with your own key and keeps a card with the account's credit balance rounded down, with the exact fractional balance as an extra, refreshed on its own.",
        card="The account's credit balance rounded down, with the exact fractional balance as an extra.",
        endpoint='`GET https://api.stability.ai/v1/user/balance`', auth='',
        docs='https://platform.stability.ai/docs/api-reference (verified against the mirror https://stabilityai.apidog.io/api-6070374)',
        cost='the docs list no price for this call; it reads the balance and generates no images.',
        note='Credits are fractional; the card rounds down and lists the exact balance.',
    ),
    dict(
        id='bfl', name='Black Forest Labs',
        head='Check your Black Forest Labs balance',
        lead="Mr. Usage reads Black Forest Labs's documented usage endpoint with your own key and keeps a card with the account's credit balance rounded down, with the exact fractional balance as an extra when it is not whole, refreshed on its own.",
        card="The account's credit balance rounded down, with the exact fractional balance as an extra when it is not whole.",
        endpoint='`GET https://api.bfl.ai/v1/credits`', auth='',
        docs='https://docs.bfl.ml/api-reference/get-the-users-credits',
        cost='the docs list no price for this call; it reads the balance and generates no images.',
        note='',
    ),
    dict(
        id='luma', name='Luma',
        head='Check your Luma balance',
        lead="Mr. Usage reads Luma's documented usage endpoint with your own key and keeps a card with the Dream Machine credit balance in US dollars (the API reports cents), refreshed on its own.",
        card='The Dream Machine credit balance in US dollars (the API reports cents).',
        endpoint='`GET https://api.lumalabs.ai/dream-machine/v1/credits`', auth='',
        docs='https://docs.lumalabs.ai/reference/getcredits',
        cost='the docs list no price for this call; it reads the balance and generates no video.',
        note="For Dream Machine API keys. Luma's newer API Platform keys are documented elsewhere.",
    ),
    dict(
        id='recraft', name='Recraft',
        head='Check what you have left at Recraft',
        lead="Mr. Usage reads Recraft's documented usage endpoint with your own key and keeps a card with the account's remaining Recraft credits, refreshed on its own.",
        card="The account's remaining Recraft credits.",
        endpoint='`GET https://external.api.recraft.ai/v1/users/me`', auth='',
        docs='https://www.recraft.ai/docs/api-reference/endpoints',
        cost='free per the docs; the endpoint table lists a price for every call and none for this one.',
        note="Free to call: Recraft's endpoint table lists no price for it.",
    ),
    dict(
        id='runpod', name='RunPod',
        head='Check your RunPod balance',
        lead="Mr. Usage reads RunPod's documented usage endpoint with your own key and keeps a card with the prepaid RunPod balance in dollars, the current hourly burn rate, and the account spend limit when one is set, refreshed on its own.",
        card='The prepaid RunPod balance in dollars, the current hourly burn rate, and the account spend limit when one is set.',
        endpoint='`POST https://api.runpod.io/graphql`', auth='',
        docs='https://graphql-spec.runpod.io/',
        cost="RunPod's pages state no price for reading the account; the query starts no pod.",
        note='Shows the prepaid balance and the current hourly burn. RunPod says this GraphQL API retires in early 2027.',
    ),
    dict(
        id='vast', name='Vast.ai',
        head='Check your Vast.ai balance',
        lead='Mr. Usage reads Vast.ai\'s documented usage endpoint with your own key and keeps a card with the account\'s prepaid balance in dollars; a negative balance shows as zero left with an "Owed" line, refreshed on its own.',
        card='The account\'s prepaid balance in dollars; a negative balance shows as zero left with an "Owed" line.',
        endpoint='`GET https://console.vast.ai/api/v0/users/current`', auth='',
        docs='https://docs.vast.ai/api-reference/accounts/show-user',
        cost='the page states no price; the call reads the account record and rents nothing.',
        note="Shows the account's prepaid balance. Vast.ai does not report a limit or a renewal date.",
    ),
    dict(
        id='digitalocean', name='DigitalOcean',
        head='Check your DigitalOcean balance',
        lead="Mr. Usage reads DigitalOcean's documented usage endpoint with your own key and keeps a card with account credit left (when the balance is negative) and this month's usage in dollars, with any amount owed and the month-to-date balance as lines, refreshed on its own.",
        card="Account credit left (when the balance is negative) and this month's usage in dollars, with any amount owed and the month-to-date balance as lines.",
        endpoint='`GET https://api.digitalocean.com/v2/customers/my/balance`', auth='',
        docs='https://docs.digitalocean.com/reference/api/digitalocean/#tag/Billing/operation/balance_get',
        cost='the page states no price; the API is rate limited (5000 requests per hour), not billed.',
        note="The token needs the billing:read scope. Shows credit left and this month's usage; a balance owed appears as a line.",
    ),
    dict(
        id='azurequantum', name='Azure Quantum',
        head='Check how much of your Azure Quantum plan is left',
        lead="Mr. Usage reads Azure Quantum's documented usage endpoint with your own key and keeps a card with the largest free job-hours quota of the workspace in seconds, what is used and held against it, when it resets, and the other quota rows (FPGA hours, concurrent jobs) as lines, refreshed on its own.",
        card='The largest free job-hours quota of the workspace in seconds, what is used and held against it, when it resets, and the other quota rows (FPGA hours, concurrent jobs) as lines.',
        endpoint='`GET <workspace address>/quotas?api-version=2026-01-15-preview`', auth='',
        docs='https://learn.microsoft.com/en-us/rest/api/azurequantum/dataplane/quotas/list?view=rest-azurequantum-dataplane-2026-01-15-preview',
        cost='the page states no price; listing quotas runs no job and Azure Quantum bills job execution.',
        note='Shows the largest job-hours quota in seconds. The key is a workspace API key, sent in the x-ms-quantum-api-key header.',
    ),
    dict(
        id='ibmquantum', name='IBM Quantum',
        head='Check what you have left at IBM Quantum',
        lead="Mr. Usage reads IBM Quantum's documented usage endpoint with your own key and keeps a card with nothing yet; the card explains that IBM Quantum needs an IAM token exchange on a second host, refreshed on its own.",
        card='Nothing yet; the card explains that IBM Quantum needs an IAM token exchange on a second host.',
        endpoint='`https://quantum.cloud.ibm.com/api/v1/instances/usage` (template with `<instance CRN>`; not called)', auth='',
        docs='https://quantum.cloud.ibm.com/docs/en/api/qiskit-runtime-rest',
        cost='not applicable (no call is made).',
        note='Not available: IBM requires an IAM token exchange on a second host, which this app does not do.',
    ),
    dict(
        id='ahrefs', name='Ahrefs',
        head='Check what you have left at Ahrefs',
        lead="Mr. Usage reads Ahrefs's documented usage endpoint with your own key and keeps a card with API units left of the key's monthly limit, or of the workspace pool when the key has no limit of its own, and the day the count resets, refreshed on its own.",
        card="API units left of the key's monthly limit, or of the workspace pool when the key has no limit of its own, and the day the count resets.",
        endpoint='`GET https://api.ahrefs.com/v3/subscription-info/limits-and-usage?output=json`', auth='`Authorization: Bearer` header.',
        docs='https://docs.ahrefs.com/en/api/reference/subscription-info/get-limits-and-usage',
        cost='the docs say the call is free and consumes no API units.',
        note="Counts API units. Shows this key's own monthly limit when it has one, otherwise the workspace's.",
    ),
    dict(
        id='moz', name='Moz',
        head='Check how much of your Moz plan is left',
        lead="Mr. Usage reads Moz's documented usage endpoint with your own key and keeps a card with rows left of the month's Data API quota, and when the period resets, refreshed on its own.",
        card="Rows left of the month's Data API quota, and when the period resets.",
        endpoint='`POST https://api.moz.com/jsonrpc` (fixed body, method `quota.lookup`)', auth='`x-moz-token` header.',
        docs='https://moz.com/api/docs/methods/QuotaLookupAction',
        cost='the docs say the lookup does not require any quota.',
        note='Counts rows of the production Data API quota (api.limits.data.rows). The lookup itself uses no quota.',
    ),
    dict(
        id='semrush', name='Semrush',
        head='Check your Semrush balance',
        lead="Mr. Usage reads Semrush's documented usage endpoint with your own key and keeps a card with standard API units left on the balance; Semrush reports no limit or renewal date, refreshed on its own.",
        card='Standard API units left on the balance; Semrush reports no limit or renewal date.',
        endpoint='`GET https://www.semrush.com/users/countapiunits.html?key=<key>`', auth='`key` query parameter (the only documented way).',
        docs='https://developer.semrush.com/api/v3/get-started/api-units-balance/',
        cost='the docs call the balance request free.',
        note='Shows the Standard API unit balance. The Trends API has its own limits that this call does not cover.',
    ),
    dict(
        id='seranking', name='SE Ranking',
        head='Check what you have left at SE Ranking',
        lead="Mr. Usage reads SE Ranking's documented usage endpoint with your own key and keeps a card with data API units left of the subscription total, with the subscription expiry as the renewal date, refreshed on its own.",
        card='Data API units left of the subscription total, with the subscription expiry as the renewal date.',
        endpoint='`GET https://api.seranking.com/v1/account/subscription?output=json`', auth='`Authorization: Token <key>` header.',
        docs='https://seranking.com/api/data/account-system/',
        cost='the docs state 0 credits.',
        note='Counts Data API units for the subscription; the renewal date is its expiration date.',
    ),
    dict(
        id='serpstat', name='Serpstat',
        head='Check how much of your Serpstat plan is left',
        lead="Mr. Usage reads Serpstat's documented usage endpoint with your own key and keeps a card with API credits (result lines) left of the plan's maximum; Serpstat reports no renewal date, refreshed on its own.",
        card="API credits (result lines) left of the plan's maximum; Serpstat reports no renewal date.",
        endpoint='`POST https://api.serpstat.com/v4/?token=<key>` (fixed body, method `SerpstatLimitsProcedure.getStats`)', auth='`token` query parameter (the only documented way).',
        docs='https://api-docs.serpstat.com/docs/serpstat-public-api/p2grcut8yurv0-get-stats',
        cost='the docs say the method does not spend API credits.',
        note='Counts API credits (result lines). The stats call itself is free.',
    ),
    dict(
        id='mangools', name='Mangools',
        head='Check what you have left at Mangools',
        lead="Mr. Usage reads Mangools's documented usage endpoint with your own key and keeps a card with SERP lookups left of the pool's limit and when it resets, with the other pools listed below, refreshed on its own.",
        card="SERP lookups left of the pool's limit and when it resets, with the other pools listed below.",
        endpoint='`GET https://api.mangools.com/v3/kwfinder/limits`', auth='`x-access-token` header.',
        docs='https://apidocs.mangools.com/kwfinder/get-limits/',
        cost='not stated by the docs; the page presents the call as the way to check quota before spending.',
        note='Headline is the SERP lookup pool. Mangools answers an unknown token with empty pools, so a bad key shows as rejected only when every pool is empty.',
    ),
    dict(
        id='dataforseo', name='DataForSEO',
        head='Check your DataForSEO balance',
        lead="Mr. Usage reads DataForSEO's documented usage endpoint with your own key and keeps a card with account balance in dollars, with total deposits, today's spend and the daily and per-minute spending limits below, refreshed on its own.",
        card="Account balance in dollars, with total deposits, today's spend and the daily and per-minute spending limits below.",
        endpoint='`GET https://api.dataforseo.com/v3/appendix/user_data`', auth='paste `login:password`; sent as `Authorization: Basic` (base64).',
        docs='https://docs.dataforseo.com/v3/appendix/user_data/',
        cost='the docs say the account is not charged for this call.',
        note='Paste the API login and password as one key in the form login:password. The balance call is free.',
    ),
    dict(
        id='majestic', name='Majestic',
        head='Check your Majestic balance',
        lead="Mr. Usage reads Majestic's documented usage endpoint with your own key and keeps a card with retrieval units left on the balance, with analysis and index-item units below; the earliest subscription expiry is the renewal date, refreshed on its own.",
        card='Retrieval units left on the balance, with analysis and index-item units below; the earliest subscription expiry is the renewal date.',
        endpoint='`GET https://api.majestic.com/api/json?cmd=GetSubscriptionInfo&app_api_key=<key>`', auth='`app_api_key` query parameter (the documented way).',
        docs='https://developer-support.majestic.com/api/commands/get-subscription-info.shtml',
        cost='the docs say the command uses no resources.',
        note='Headline is retrieval units; analysis and index-item units are listed below. The renewal date is the earliest subscription expiry.',
    ),
    dict(
        id='keywordseverywhere', name='Keywords Everywhere',
        head='Check your Keywords Everywhere balance',
        lead="Mr. Usage reads Keywords Everywhere's documented usage endpoint with your own key and keeps a card with prepaid credits left on the balance; the vendor reports no limit or renewal date, refreshed on its own.",
        card='Prepaid credits left on the balance; the vendor reports no limit or renewal date.',
        endpoint='`GET https://api.keywordseverywhere.com/v1/account/credits`', auth='`Authorization: Bearer` header.',
        docs='https://api.keywordseverywhere.com/docs/miscellaneous/get_credits.md',
        cost='not stated by the docs.',
        note='Credits are prepaid and do not reset, so there is no renewal date.',
    ),
]
# fmt: on

STYLE = """
  :root { color-scheme: dark; --ink:#f0efec; --sub:#c3c2b7; --line:#383835; --accent:#86b6ef; --bg:#1a1a19; --code:#232321; }
  * { box-sizing: border-box; }
  body { margin:0; background:var(--bg); color:var(--ink); font:17px/1.65 system-ui, -apple-system, "Segoe UI", "Cantarell", "Noto Sans", sans-serif; }
  main { max-width: 46rem; margin: 0 auto; padding: 3rem 1.25rem 4rem; }
  h1 { font-size: 2rem; line-height:1.2; margin: 0 0 .5rem; }
  h2 { font-size: 1.3rem; margin: 2.2rem 0 .6rem; }
  p { margin: .7rem 0; }
  .crumbs { color: var(--sub); font-size: .9rem; margin: 0 0 1.5rem; }
  .sub { color: var(--sub); font-size: 1.05rem; }
  a { color: var(--accent); }
  pre { background: var(--code); padding: .9rem 1rem; border-radius: 8px; overflow-x:auto; font-size:.85rem; line-height:1.5; }
  code { background: var(--code); padding:.1rem .3rem; border-radius:4px; font-size:.9em; }
  pre code { padding:0; background:none; }
  table { border-collapse: collapse; width:100%; font-size:.92rem; }
  th, td { text-align:left; padding:.45rem .6rem; border-bottom:1px solid var(--line); vertical-align: top; }
  .cta { display:inline-block; margin:.8rem .6rem 0 0; padding:.55rem 1rem; border:1px solid var(--accent); border-radius:8px; text-decoration:none; font-weight:600; }
  .others { display:flex; flex-wrap:wrap; gap:.4rem .6rem; padding:0; list-style:none; margin:.4rem 0 0; }
  .others li a { display:inline-block; padding:.25rem .7rem; border:1px solid var(--line); border-radius:999px; text-decoration:none; font-size:.9rem; }
  footer { margin-top:3rem; padding-top:1rem; border-top:1px solid var(--line); color:var(--sub); font-size:.9rem; }
"""


def esc(s: str) -> str:
    return html.escape(s, quote=True)


def faq(p: dict) -> list[tuple[str, str]]:
    return [
        (f"Does checking my {p['name']} usage cost credits?", p["cost"]),
        (
            f"Where does my {p['name']} API key go?",
            "Into your operating system's credential store, under the service name mr-usage: Keychain on macOS, Credential Manager on Windows, the Secret Service on Linux. The app reads it at the moment it calls the vendor, wraps it in a zeroizing type, and drops it. It is never written to a file of the app's own and never sent back to the window.",
        ),
        (
            "How often is the card refreshed?",
            "Every 5 minutes by default. Settings lets you pick anything from 1 minute to 24 hours, and changing the interval takes effect from the last poll. There is also a Refresh button on each card, limited to once a minute.",
        ),
    ]


def page(p: dict) -> str:
    url = f"{SITE}providers/{p['id']}.html"
    title = f"{p['head']}: Mr. Usage"
    description = f"{p['lead']} Open source for macOS, Windows and Linux. Keys stay in your OS keychain."
    questions = faq(p)
    ld = [
        {
            "@context": "https://schema.org",
            "@type": "SoftwareApplication",
            "name": "Mr. Usage",
            "applicationCategory": "UtilitiesApplication",
            "operatingSystem": "macOS, Windows, Linux",
            "offers": {"@type": "Offer", "price": "0", "priceCurrency": "USD"},
            "url": SITE,
            "downloadUrl": f"{REPO}/releases/latest",
            "softwareHelp": url,
            "license": f"{REPO}/blob/HEAD/LICENSE-MIT",
            "author": {"@type": "Person", "name": "Alvee Rahman", "url": "https://github.com/AlveeeRahman"},
        },
        {
            "@context": "https://schema.org",
            "@type": "FAQPage",
            "mainEntity": [
                {"@type": "Question", "name": q, "acceptedAnswer": {"@type": "Answer", "text": a}} for q, a in questions
            ],
        },
        {
            "@context": "https://schema.org",
            "@type": "BreadcrumbList",
            "itemListElement": [
                {"@type": "ListItem", "position": 1, "name": "Mr. Usage", "item": SITE},
                {"@type": "ListItem", "position": 2, "name": p["name"], "item": url},
            ],
        },
    ]
    others = "".join(
        f'<li><a href="{o["id"]}.html">{esc(o["name"])}</a></li>' for o in PROVIDERS if o["id"] != p["id"]
    )
    note = f"<p>{esc(p['note'])}</p>" if p["note"] else ""
    faq_html = "".join(f"<h3>{esc(q)}</h3><p>{esc(a)}</p>" for q, a in questions)
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(title)}</title>
<meta name="description" content="{esc(description)}">
<link rel="canonical" href="{url}">
<meta property="og:type" content="article">
<meta property="og:site_name" content="Mr. Usage">
<meta property="og:title" content="{esc(title)}">
<meta property="og:description" content="{esc(description)}">
<meta property="og:url" content="{url}">
<meta property="og:image" content="{SITE}assets/social-preview.png">
<meta property="og:image:width" content="1280">
<meta property="og:image:height" content="640">
<meta property="og:image:alt" content="Mr. Usage: remaining API credits for nine vendors in one desktop window">
<meta name="twitter:card" content="summary_large_image">
<script type="application/ld+json">
{json.dumps(ld, indent=2)}
</script>
<style>{STYLE}</style>
</head>
<body>
<main>
  <p class="crumbs"><a href="../">Mr. Usage</a> &rsaquo; {esc(p['name'])}</p>
  <h1>{esc(p['head'])}</h1>
  <p class="sub">{esc(p['lead'])}</p>

  <p>
    <a class="cta" href="../#install">Install Mr. Usage</a>
    <a class="cta" href="{REPO}">GitHub repository</a>
  </p>

  <h2>What the {esc(p['name'])} card shows</h2>
  <p>{esc(p['card'])} A yellow "Low" badge appears under 20% left and a red "Nearly out" badge under 5%, with a line of today's readings underneath.</p>
  {note}

  <h2>What Mr. Usage reads</h2>
  <table>
    <tr><th>Call</th><td><code>{esc(p['endpoint'])}</code></td></tr>
    <tr><th>Authentication</th><td>{esc(p['auth'])}</td></tr>
    <tr><th>Verified against</th><td><a href="{esc(p['docs'])}" rel="noopener">{esc(p['docs'])}</a></td></tr>
    <tr><th>Anything else contacted</th><td>Nothing. The host is on a per-provider allowlist checked before every request, redirects are off, and the connection is TLS 1.2 or newer.</td></tr>
  </table>
  <p>The app shows you this exact address before you save a key, and <a href="{REPO}/blob/main/docs/SOURCES.md">docs/SOURCES.md</a> records where each fact came from and when it was read.</p>

  <h2>Questions</h2>
  {faq_html}

  <h2>Other providers</h2>
  <ul class="others">{others}</ul>

  <footer>
    Open source under MIT or Apache-2.0 &middot; <a href="{REPO}">github.com/AlveeeRahman/mr-usage</a> &middot; <a href="../">All providers and install</a>
  </footer>
</main>
</body>
</html>
"""


def sitemap() -> str:
    urls = [SITE] + [f"{SITE}providers/{p['id']}.html" for p in PROVIDERS]
    items = "".join(f"  <url>\n    <loc>{u}</loc>\n    <lastmod>{TODAY}</lastmod>\n  </url>\n" for u in urls)
    return f'<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n{items}</urlset>\n'


def main() -> None:
    out = ROOT / "docs" / "providers"
    out.mkdir(parents=True, exist_ok=True)
    for p in PROVIDERS:
        (out / f"{p['id']}.html").write_text(page(p), encoding="utf-8")
    (ROOT / "docs" / "sitemap.xml").write_text(sitemap(), encoding="utf-8")
    print(f"wrote {len(PROVIDERS)} provider pages and docs/sitemap.xml")


if __name__ == "__main__":
    main()
