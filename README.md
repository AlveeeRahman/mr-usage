# Mr. Usage

One window that shows how much of each paid API you have left. Paste a key once. After that the app keeps the number current on its own.

It works with Firecrawl, SerpApi, Elicit, Context7, Tavily, OpenRouter, Apify, Hunter and Runway, on macOS, Windows and Linux.

![Mr. Usage showing remaining credits for several providers in dark mode](docs/assets/social-preview.png)

Your API keys go into the credential store your operating system already has (Keychain on macOS, Credential Manager on Windows, GNOME Keyring or KWallet on Linux). The app has no account, no server and no analytics. The only thing it talks to is the usage page of each vendor you gave it a key for.

## Install

macOS or Linux, in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/AlveeeRahman/mr-usage/main/install.sh | sh
```

Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/AlveeeRahman/mr-usage/main/install.ps1 | iex
```

The command downloads the right package for your machine, checks it against the release's checksum list, installs it, and clears the "downloaded from the internet" flag so the app opens without a detour through security settings. Then open Mr. Usage from Launchpad, the Start menu or your application menu.

<details>
<summary>Install by hand instead</summary>

Download from [Releases](https://github.com/AlveeeRahman/mr-usage/releases/latest):

| Your computer | File |
| --- | --- |
| Mac with Apple silicon (M1 or later) | `..._aarch64.dmg` |
| Mac with an Intel chip | `..._x64.dmg` |
| Windows | `..._x64-setup.exe` (installs for your user, no admin needed) |
| Debian, Ubuntu, Mint | `..._amd64.deb` |
| Fedora, RHEL | `....x86_64.rpm` |
| Other Linux | `..._amd64.AppImage` (needs FUSE) |

The builds are not yet signed with an Apple or Microsoft certificate, so each system asks once:

- macOS says it cannot verify the developer. Open System Settings, click Privacy & Security, scroll down and click Open Anyway, as Apple describes in [Safely open apps on your Mac](https://support.apple.com/en-us/102445).
- Windows shows a SmartScreen notice. Click "More info", then "Run anyway".
- Linux installs with `sudo apt install ./<file>.deb`, `sudo dnf install ./<file>.rpm`, or `chmod +x <file>.AppImage` and run it.

</details>

## First run

1. Open Mr. Usage. The "Add provider" panel is open on first run.
2. Pick a provider, paste its API key and press Save key. The panel shows the exact web address the app will call for that provider. The field clears as soon as the key is saved.
3. A card appears and fills in within about 15 seconds: what is left, out of what, when the plan renews, and a line showing today's readings.

Repeat for each provider you use. A card gets a yellow "Low" badge under 20% left and a red "Nearly out" badge under 5%.

| Provider | What the card shows |
| --- | --- |
| Firecrawl | Credits left out of the plan, and the end of the billing period |
| SerpApi | Searches left this month, and the renewal date |
| Elicit | Percentage of the plan left |
| Context7 | Requests left this month, and extra-request cost on paid plans |
| Tavily | Credits left out of the plan, plus this key's own usage |
| OpenRouter | Dollars left on the key's credit limit (a key without a limit shows the workspace budget, or else the month's spend) |
| Apify | Dollars left of this month's spending limit |
| Hunter | Searches left, plus verifications and the credit pool where the plan has one |
| Runway | The organization's credit balance |

Each card is refreshed every 5 minutes, and Settings lets you pick anything from 1 minute to 24 hours. Every one of these calls reads your account's own usage page. SerpApi, Context7 and Hunter document the call as free, Firecrawl's open-source server shows it bills nothing, and none of the other vendors list it as a billable call.

## What it keeps, and where

| | Where | Until |
| --- | --- | --- |
| API keys | Your system credential store, under the name `mr-usage` | You press Forget key, or uninstall |
| Settings (refresh interval, theme, which cards are on) | One small `settings.json` | You uninstall |
| Usage numbers and today's history | Memory only | Midnight, Wipe history now, or quitting the app |

Nothing else is written. The settings file never contains a key, an email address or a usage number.

## Uninstall

macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/AlveeeRahman/mr-usage/main/install.sh | sh -s -- --uninstall
```

Windows:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/AlveeeRahman/mr-usage/main/install.ps1))) -Uninstall
```

This removes the keys from your credential store, then the app, then its settings. Uninstalling through the system's own "Remove program" leaves the keys in place (operating systems treat them as your data, not the app's), so press Forget key on each card first if you go that way.

## Command line

The app takes three options and nothing else:

```
mr-usage                 open the window
mr-usage --forget-keys   delete every key it stored, then exit (the uninstallers run this)
mr-usage --help          options, exit codes and the MR_USAGE_LOG variable
mr-usage --version
```

Anything else is refused with exit status 2. On Windows the release build has no console, so `--help` prints nothing there; the exit code still applies.

## If something looks wrong

- "No Secret Service is running" (Linux): the app needs GNOME Keyring, KWallet or KeePassXC. The `.deb` and `.rpm` pull in GNOME Keyring on install. On other setups, install one and log out and in again.
- "The vendor rejected this API key": the key is wrong, expired or for a different product. Paste a fresh one from the vendor's dashboard.
- "Rate limited by the vendor": the card shows the vendor's own message and tries again later on its own.
- Elicit needs a Pro plan or above for API access. Below that, the card says so.
- The AppImage does nothing when double-clicked: install FUSE (`fuse3` on most distributions), or use the `.deb` or `.rpm`.

## More

- How the app protects keys, and what it cannot protect you from: [SECURITY.md](SECURITY.md) and [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).
- Building from source, running the tests, adding a provider: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
- Where every vendor fact in the app came from: [docs/SOURCES.md](docs/SOURCES.md).

By the author of [LLM Dogfight](https://alveeerahman.github.io/llm-dogfight/), [Skill Vision](https://alveeerahman.github.io/skill-vision/), [Agent Oracle](https://alveeerahman.github.io/agent-oracle/) and [Research Hound](https://alveeerahman.github.io/research-hound/).

## License

MIT or Apache-2.0, your choice. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
