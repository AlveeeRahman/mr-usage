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
