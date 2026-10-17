# Changelog

What changed in each release, for people deciding whether to update. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the release workflow copies a version's section into the GitHub release notes, so a tag without a section here does not release.

## [Unreleased]

### Added

- Fuzz targets for every provider parser, the shared error-body reader and the redaction rules, under `fuzz/`. CI runs each for a minute under AddressSanitizer on every push.
- Release signing: `SHA256SUMS.txt` is signed with Sigstore through the workflow's identity, every installer carries GitHub build provenance, and a `sign-release` workflow signs releases published before this. `SECURITY.md` shows how to verify both.
- Dependabot opens one grouped pull request per ecosystem each Monday.
- An OpenSSF Scorecard workflow and README badge.
- `osv-scanner.toml`, listing the ignored advisories with the same reasons as `deny.toml` and `.cargo/audit.toml`.
- `CONTRIBUTING.md` and this changelog.

### Changed

- The README and the site describe every provider card in plain words: what the main number is, and what the details behind the More button hold.
- Release notes come from this file instead of a fixed sentence.
- The social preview image names sixteen of the providers.

### Fixed

- Redaction: a URL nested inside another URL's path or query kept its userinfo and query through one scrub. Nested URLs are now scrubbed on their own.
- Redaction: scrubbing an already scrubbed URL appended a `]` on every pass.
- Installer smoke tests wait for WebKit helper processes on Linux, WebView2 helpers on Windows and the AppImage FUSE mount before judging that the app has stopped.
- The Linux end-to-end test turns off WebKitGTK's DMABUF renderer under the virtual display, which was ending the WebDriver session at random.

### Security

- Three unmaintained-crate advisories for the zbus 5 crates behind the Linux keyring store (`RUSTSEC-2026-0370`, `-0371`, `-0372`) are ignored with reasons until a release of `secret-service` builds on zbus 6.

## [1.0.0] - 2026-10-17

### Added

- Thirty-five providers, for forty-four in all.
  - LLM inference: DeepSeek, Moonshot (Kimi), Vercel AI Gateway, Novita AI, fal.ai, SiliconFlow and xAI.
  - Search and scraping: Brave Search, SearchApi.io, ScrapingBee, Bright Data, Linkup and Crawlbase.
  - Compute and quantum: RunPod, Vast.ai, DigitalOcean and Azure Quantum. IBM Quantum is listed with a card that explains why it cannot be read.
  - Speech and media: ElevenLabs, Deepgram, D-ID, HeyGen, Stability AI, Black Forest Labs, Luma and Recraft.
  - SEO and marketing: Ahrefs, Moz, Semrush, SE Ranking, Serpstat, Mangools, DataForSEO, Majestic and Keywords Everywhere.
- Two views (pinned and all APIs), category groups, and three card shapes: a quota counting down from a limit, a plain balance, and spend against a cap.
- The window remembers its size and position.
- A reading can override the provider's unit, for vendors that bill each account in its own currency.
- One page per provider on the documentation site, with the exact endpoint the app reads.

### Changed

- The release workflow creates the draft release once and every builder uploads into it by id, then publishes it from the pipeline.
- Firecrawl's usage call is confirmed free on a live account.

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
