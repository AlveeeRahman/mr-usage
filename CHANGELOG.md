# Changelog

What changed in each release. The release notes on GitHub are copied from here.

## [Unreleased]

### Added

- Signed downloads. The checksum list of every release is signed, and each installer carries a build attestation. SECURITY.md shows how to verify a download.

### Fixed

- A web address nested inside another one could reach the log with its credentials and query string intact.

## [1.0.0] - 2026-10-17

### Added

- Thirty-five more providers, forty-four in all.
  - LLM inference: DeepSeek, Moonshot (Kimi), Vercel AI Gateway, Novita AI, fal.ai, SiliconFlow and xAI.
  - Search and scraping: Brave Search, SearchApi.io, ScrapingBee, Bright Data, Linkup and Crawlbase.
  - Compute and quantum: RunPod, Vast.ai, DigitalOcean and Azure Quantum.
  - Speech and media: ElevenLabs, Deepgram, D-ID, HeyGen, Stability AI, Black Forest Labs, Luma and Recraft.
  - SEO and marketing: Ahrefs, Moz, Semrush, SE Ranking, Serpstat, Mangools, DataForSEO, Majestic and Keywords Everywhere.
- A Pinned view and an All APIs view, with cards grouped by category.
- Balances in yuan and euro for vendors that bill in them.
- The window remembers its size and position.

### Changed

- Firecrawl's usage check is confirmed to cost no credits.

## [0.1.0] - 2026-10-17

First release.

- Nine providers: Firecrawl, SerpApi, Elicit, Context7, Tavily, OpenRouter, Apify, Hunter and Runway.
- A core crate with the providers, the operating-system keystore, log redaction, an HTTP client with a per-provider host allowlist, the poller and the midnight wipe. Plain Cargo, no Tauri, so it can be tested and audited without a webview.
- The Tauri window and frontend.
- CI on Linux, macOS and Windows, an end-to-end test that drives the real Linux build through WebDriver, installer smoke tests for every package, and the release workflow.
- The threat model and the sources log.

[Unreleased]: https://github.com/AlveeeRahman/mr-usage/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/AlveeeRahman/mr-usage/compare/v0.1.0...v1.0.0
[0.1.0]: https://github.com/AlveeeRahman/mr-usage/releases/tag/v0.1.0
