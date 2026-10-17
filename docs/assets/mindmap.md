---
markmap:
  colorFreezeLevel: 2
  initialExpandLevel: -1
  maxWidth: 320
---

# Mr. Usage: desktop API credit monitor

## Providers, one documented usage endpoint each
- Firecrawl: credits left of plan, billing period end
- SerpApi: searches left, renewal date
- Elicit: percent of plan left (Pro plan or above)
- Context7: requests left, extra-request cost
- Tavily: credits left, this key's usage
- OpenRouter: dollars left on the key's limit
- Apify: dollars left of monthly limit
- Hunter: searches, verifications, credit pool
- Runway: organization credit balance

## Keys and secrets
- OS credential store under service "mr-usage"
  - macOS Keychain
  - Windows Credential Manager
  - Linux Secret Service (GNOME Keyring, KWallet, KeePassXC)
- Read at poll time, wrapped in SecretString, zeroized after
- Never written to a file, never sent back to the window
- Log redaction: URLs, bearer tokens, key-shaped strings

## Network
- HTTPS only, TLS 1.2+, rustls with ring
- Per-provider host allowlist checked before every request
- Redirects off, 64 KiB body cap, zeroized buffers
- No telemetry, no update check

## Poller
- Every 5 min by default, 1 min to 24 h
- Due providers polled at the same time
- Backoff on failure, stop on a rejected key
- Manual refresh, once a minute
- History in memory, wiped at midnight and on quit

## Window (Tauri 2, TypeScript, no framework)
- One card per provider: remaining, limit, renewal
- Low under 20%, Nearly out under 5%
- Sparkline of today's readings
- Settings: interval, theme, wipe history
- Strict CSP, eight commands, no network from the page

## Command line
- mr-usage opens the window
- --forget-keys, run by the uninstallers
- --help, --version
- Exit codes 0, 1, 2

## Distribution
- macOS .dmg (Apple silicon, Intel)
- Windows NSIS .exe and .msi
- Linux .deb, .rpm, AppImage
- install.sh and install.ps1 verify SHA256SUMS
- Release workflow builds, drafts, test-installs

## Checks on every push
- fmt, clippy -D warnings, 109 tests on 3 OSes
- Rust 1.90 minimum version
- Real keystore and core-dump leak scan
- End to end through WebDriver
- cargo deny, cargo audit, npm audit
- Installer smoke tests on each OS

## Layout
- crates/core: providers, keystore, redaction, poller
- src-tauri: shell, commands, capabilities
- frontend: the window
- installers: install.sh, install.ps1
- docs: threat model, sources, Pages site
