#!/bin/sh
# Installs Mr. Usage on macOS or Linux, or removes it.
#
#   curl -fsSL https://raw.githubusercontent.com/AlveeeRahman/mr-usage/main/installers/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/AlveeeRahman/mr-usage/main/installers/install.sh | sh -s -- --uninstall
#
# What it does: picks the right package for this machine, downloads it from
# GitHub Releases together with the release's SHA256SUMS.txt, refuses to
# install anything whose checksum is not on that list, installs it, and on
# macOS clears the quarantine flag so the app opens without a Gatekeeper
# detour. `--uninstall` first asks the app to delete the API keys it stored in
# your credential store, then removes the app and its settings folder.
#
# Options:
#   --version vX.Y.Z   a specific release instead of the latest
#   --appimage         Linux: the AppImage instead of the .deb or .rpm
#   --from DIR         use package files already in DIR (no download)
#   --uninstall        remove Mr. Usage and the keys it stored
#   --help
set -eu

REPO="AlveeeRahman/mr-usage"
APP_NAME="Mr. Usage"
ID="io.github.alveeerahman.mr-usage"
VERSION=""
FROM=""
WANT_APPIMAGE=0
UNINSTALL=0

say() { printf '%s\n' "$*"; }
die() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:?--version needs a value}"; shift ;;
    --version=*) VERSION="${1#*=}" ;;
    --appimage) WANT_APPIMAGE=1 ;;
    --from) FROM="${2:?--from needs a directory}"; shift ;;
    --from=*) FROM="${1#*=}" ;;
    --uninstall) UNINSTALL=1 ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
  shift
done

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$ARCH" in
  arm64|aarch64) ARCH=aarch64 ;;
  x86_64|amd64) ARCH=x86_64 ;;
esac

SUDO=""
if [ "$(id -u)" != "0" ] && command -v sudo >/dev/null 2>&1; then SUDO="sudo"; fi

# ---------------------------------------------------------------- choose ---
# KIND is one of dmg, deb, rpm, appimage. SUFFIX is the tail of the asset name
# that identifies it among the release files.
case "$OS" in
  Darwin)
    KIND=dmg
    [ "$ARCH" = aarch64 ] && SUFFIX="_aarch64.dmg" || SUFFIX="_x64.dmg"
    ;;
  Linux)
    [ "$ARCH" = x86_64 ] || die "the Linux builds are x86_64 only; this machine is $ARCH"
    if [ "$WANT_APPIMAGE" = 1 ]; then KIND=appimage; SUFFIX="_amd64.AppImage"
    elif command -v apt-get >/dev/null 2>&1 && command -v dpkg >/dev/null 2>&1; then KIND=deb; SUFFIX="_amd64.deb"
    elif command -v dnf >/dev/null 2>&1; then KIND=rpm; SUFFIX=".x86_64.rpm"
    else KIND=appimage; SUFFIX="_amd64.AppImage"
    fi
    ;;
  *) die "unsupported OS: $OS (macOS and Linux only; Windows uses install.ps1)" ;;
esac

# ------------------------------------------------------------- uninstall ---
forget_keys() { # path-to-binary
  if [ -x "$1" ]; then
    if "$1" --forget-keys; then say "API keys removed from the credential store."
    else say "Could not reach the credential store, so any stored keys were left in place (see the message above)."; fi
  else
    say "Mr. Usage is not installed, so there are no keys to remove through it."
  fi
}

if [ "$UNINSTALL" = 1 ]; then
  case "$OS" in
    Darwin)
      forget_keys "/Applications/$APP_NAME.app/Contents/MacOS/mr-usage"
      rm -rf "/Applications/$APP_NAME.app"
      rm -rf "$HOME/Library/Application Support/$ID" "$HOME/Library/Caches/$ID" "$HOME/Library/WebKit/$ID"
      ;;
    Linux)
      if [ -x /usr/bin/mr-usage ]; then
        forget_keys /usr/bin/mr-usage
        if command -v dpkg >/dev/null 2>&1 && dpkg -s mr-usage >/dev/null 2>&1; then $SUDO apt-get remove -y mr-usage
        elif command -v rpm >/dev/null 2>&1 && rpm -q mr-usage >/dev/null 2>&1; then $SUDO dnf remove -y mr-usage
        fi
      elif [ -x "$HOME/.local/bin/mr-usage" ]; then
        forget_keys "$HOME/.local/bin/mr-usage"
        rm -f "$HOME/.local/bin/mr-usage" "$HOME/.local/share/applications/mr-usage.desktop"
      else
        say "Mr. Usage is not installed."
      fi
      rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/$ID"
      ;;
  esac
  say "Mr. Usage removed."
  exit 0
fi

# -------------------------------------------------------------- download ---
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  else die "need sha256sum or shasum to verify the download"; fi
}

if [ -n "$FROM" ]; then
  # Absolute, because apt-get only treats a path as a file when it starts with / or ./
  FROM="$(cd "$FROM" && pwd)" || die "no such directory: $FROM"
  PKG="$(find "$FROM" -maxdepth 1 -name "*$SUFFIX" | head -n 1)"
  [ -n "$PKG" ] || die "no *$SUFFIX in $FROM"
  SUMS="$FROM/SHA256SUMS.txt"
elif [ -n "${GH_TOKEN:-}" ] && command -v gh >/dev/null 2>&1; then
  # Maintainers and the release workflow: `gh` can read a draft release,
  # which the public download URLs cannot. Users never take this branch.
  [ -n "$VERSION" ] || die "set --version when installing through gh"
  say "Downloading $VERSION through gh..."
  gh release download "$VERSION" --repo "$REPO" --dir "$TMP" --pattern "*$SUFFIX" --pattern "SHA256SUMS.txt" || die "gh could not download release $VERSION"
  PKG="$(find "$TMP" -maxdepth 1 -name "*$SUFFIX" | head -n 1)"
  [ -n "$PKG" ] || die "release $VERSION has no *$SUFFIX file"
  SUMS="$TMP/SHA256SUMS.txt"
  [ -f "$SUMS" ] || die "release $VERSION has no SHA256SUMS.txt, so the download cannot be verified; nothing installed"
else
  command -v curl >/dev/null 2>&1 || die "curl is required"
  if [ -n "$VERSION" ]; then API="https://api.github.com/repos/$REPO/releases/tags/$VERSION"
  else API="https://api.github.com/repos/$REPO/releases/latest"; fi
  say "Looking up the release..."
  curl -fsSL -H "Accept: application/vnd.github+json" "$API" >"$TMP/release.json" || die "could not read $API (no release published yet, or no network)"
  URLS="$(grep -o '"browser_download_url": *"[^"]*"' "$TMP/release.json" | sed 's/.*"\(https[^"]*\)"/\1/')"
  PKG_URL="$(printf '%s\n' "$URLS" | grep "$SUFFIX\$" | head -n 1)"
  SUMS_URL="$(printf '%s\n' "$URLS" | grep '/SHA256SUMS.txt$' | head -n 1)"
  [ -n "$PKG_URL" ] || die "this release has no *$SUFFIX file"
  [ -n "$SUMS_URL" ] || die "this release has no SHA256SUMS.txt, so the download cannot be verified; nothing installed"
  PKG="$TMP/package$SUFFIX"
  SUMS="$TMP/SHA256SUMS.txt"
  say "Downloading $(basename "$PKG_URL")..."
  curl -fsSL -o "$PKG" "$PKG_URL"
  curl -fsSL -o "$SUMS" "$SUMS_URL"
fi

# ---------------------------------------------------------------- verify ---
if [ -f "$SUMS" ]; then
  HASH="$(sha256_of "$PKG")"
  if grep -qi "^$HASH " "$SUMS"; then say "Checksum verified."
  else die "the download's SHA-256 ($HASH) is not in SHA256SUMS.txt; nothing installed"; fi
elif [ -n "$FROM" ]; then
  say "No SHA256SUMS.txt next to the package; skipping the checksum (local files)."
fi

# --------------------------------------------------------------- install ---
case "$KIND" in
  dmg)
    MNT="$TMP/mnt"
    mkdir -p "$MNT"
    hdiutil attach -nobrowse -readonly -mountpoint "$MNT" "$PKG" >/dev/null
    rm -rf "/Applications/$APP_NAME.app"
    cp -R "$MNT/$APP_NAME.app" /Applications/
    hdiutil detach "$MNT" >/dev/null
    # The build is not notarized. Clearing the quarantine flag is what
    # "Open Anyway" in System Settings does; the checksum above stands in for
    # the signature check Gatekeeper would otherwise do.
    xattr -dr com.apple.quarantine "/Applications/$APP_NAME.app" 2>/dev/null || true
    say "Installed /Applications/$APP_NAME.app. Open it from Launchpad or Spotlight."
    ;;
  deb)
    # apt pulls in gnome-keyring (a Recommends) unless the system turned
    # recommends off, in which case the app says what is missing on first run.
    $SUDO apt-get install -y "$PKG"
    say "Installed. Find $APP_NAME in your application menu, or run mr-usage."
    ;;
  rpm)
    $SUDO dnf install -y "$PKG"
    say "Installed. Find $APP_NAME in your application menu, or run mr-usage."
    ;;
  appimage)
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
    install -m 755 "$PKG" "$HOME/.local/bin/mr-usage"
    cat >"$HOME/.local/share/applications/mr-usage.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Comment=Remaining API credits at a glance
Exec=$HOME/.local/bin/mr-usage
Categories=Utility;
Terminal=false
EOF
    say "Installed to ~/.local/bin/mr-usage with a menu entry. It needs FUSE (package fuse3 or fuse) to run."
    ;;
esac

if [ "$OS" = Linux ] && ! command -v gnome-keyring-daemon >/dev/null 2>&1 && ! command -v kwalletd6 >/dev/null 2>&1 && ! command -v kwalletd5 >/dev/null 2>&1 && ! command -v keepassxc >/dev/null 2>&1; then
  say "Note: no Secret Service provider was found (GNOME Keyring, KWallet or KeePassXC). Mr. Usage needs one to store keys."
fi
