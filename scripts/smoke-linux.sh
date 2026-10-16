#!/usr/bin/env bash
# Install, start, stop and uninstall smoke test for the Linux packages.
#
#   scripts/smoke-linux.sh deb      <path/to/mr-usage_*.deb>
#   scripts/smoke-linux.sh appimage <path/to/Mr. Usage_*.AppImage>
#   scripts/smoke-linux.sh rpm      <path/to/mr-usage-*.rpm>     (Fedora/RHEL)
#
# For each package it checks that:
#   - it installs, and puts the binary, desktop entry and icons in place;
#   - the app starts, shows its window and spawns its WebKit helpers;
#   - it stops cleanly three ways (window close, SIGTERM, SIGINT): exit code
#     0 within 10 s, no helper processes left, no webview data left behind;
#   - it starts again after each stop;
#   - it writes nothing outside its own config folder (settings.json);
#   - it uninstalls and leaves none of its files behind.
#
# Runs with a throwaway HOME, XDG dirs, display, D-Bus session and keyring.
# Needs: xvfb, openbox, wmctrl, xdotool, dbus, gnome-keyring, and root (or
# sudo) for deb/rpm installs.
set -euo pipefail

KIND="${1:?deb|appimage|rpm}"
PKG="$(realpath "${2:?package path}")"
SUDO=""
[ "$(id -u)" = "0" ] || SUDO="sudo"
FAILS=0
pass() { echo "PASS $*"; }
fail() { echo "FAIL $*"; FAILS=$((FAILS + 1)); }

SANDBOX="$(mktemp -d)"
export HOME="$SANDBOX/home" XDG_CONFIG_HOME="$SANDBOX/home/.config" XDG_DATA_HOME="$SANDBOX/home/.local/share" \
  XDG_CACHE_HOME="$SANDBOX/home/.cache" XDG_STATE_HOME="$SANDBOX/home/.local/state" XDG_RUNTIME_DIR="$SANDBOX/run"
mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
export DISPLAY=:97
Xvfb :97 -screen 0 1280x900x24 >/dev/null 2>&1 &
XVFB=$!
sleep 1
openbox >/dev/null 2>&1 &
WM=$!
cleanup() {
  kill "$WM" "$XVFB" 2>/dev/null || true
  rm -rf "$SANDBOX"
}
trap cleanup EXIT

case "$KIND" in
  deb)
    $SUDO apt-get install -y "$PKG" >/dev/null
    mapfile -t FILES < <(dpkg -L mr-usage | while read -r f; do [ -f "$f" ] && echo "$f"; done)
    BIN=/usr/bin/mr-usage
    ;;
  rpm)
    $SUDO dnf install -y "$PKG" >/dev/null
    mapfile -t FILES < <(rpm -ql mr-usage | while read -r f; do [ -f "$f" ] && echo "$f"; done)
    BIN=/usr/bin/mr-usage
    ;;
  appimage)
    chmod +x "$PKG"
    FILES=("$PKG")
    BIN="$PKG"
    # No FUSE in containers and CI: run from a temporary extraction, which
    # the AppImage runtime must clean up on exit.
    export APPIMAGE_EXTRACT_AND_RUN=1
    ;;
  *) echo "unknown kind $KIND"; exit 2 ;;
esac

if [ "$KIND" != appimage ]; then
  [ -x "$BIN" ] && pass "binary installed at $BIN" || fail "binary missing at $BIN"
  printf '%s\n' "${FILES[@]}" | grep -q '\.desktop$' && pass "desktop entry installed" || fail "no desktop entry"
  printf '%s\n' "${FILES[@]}" | grep -q '/icons/' && pass "icons installed" || fail "no icons"
fi

helpers() { pgrep -c -f 'WebKit(Web|Network)Process' || true; }
leftover_webview() { find "$XDG_RUNTIME_DIR" -maxdepth 1 -name 'mr-usage-webview-*' | wc -l; }

# One start/stop cycle. $1 = close | term | int
cycle() {
  local how="$1" pid win code start elapsed
  "$BIN" >"$SANDBOX/app-$how.log" 2>&1 &
  pid=$!
  win=""
  for _ in $(seq 1 60); do
    win="$(xdotool search --name '^Mr\. Usage$' 2>/dev/null | head -n1 || true)"
    [ -n "$win" ] && break
    sleep 0.5
  done
  if [ -z "$win" ]; then
    fail "[$how] window did not appear"
    kill -KILL "$pid" 2>/dev/null || true
    return
  fi
  sleep 2
  [ "$(helpers)" -ge 1 ] && pass "[$how] started: window up, WebKit helpers running" || fail "[$how] no WebKit helpers"
  [ "$(leftover_webview)" = "1" ] && pass "[$how] ephemeral webview folder created in XDG_RUNTIME_DIR" || fail "[$how] no ephemeral webview folder"

  start=$(date +%s)
  case "$how" in
    close) wmctrl -c 'Mr. Usage' ;;
    term) kill -TERM "$pid" ;;
    int) kill -INT "$pid" ;;
  esac
  code=0
  for _ in $(seq 1 100); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "$pid" 2>/dev/null; then
    fail "[$how] still running 10 s after stop request"
    kill -KILL "$pid" 2>/dev/null || true
  fi
  wait "$pid" || code=$?
  elapsed=$(($(date +%s) - start))
  [ "$code" = "0" ] && pass "[$how] exited with code 0 in ${elapsed}s" || fail "[$how] exit code $code"
  sleep 1
  [ "$(helpers)" = "0" ] && pass "[$how] no WebKit helpers left" || fail "[$how] $(helpers) WebKit helper(s) left running"
  [ "$(leftover_webview)" = "0" ] && pass "[$how] webview folder removed" || fail "[$how] webview folder left behind"
  if [ "$KIND" = appimage ]; then
    local extracted
    extracted="$(find /tmp "$SANDBOX" -maxdepth 1 -name 'appimage_extracted_*' 2>/dev/null | wc -l)"
    [ "$extracted" = "0" ] && pass "[$how] AppImage extraction cleaned up" || fail "[$how] AppImage extraction left in /tmp"
  fi
  if grep -i -E 'panic|error' "$SANDBOX/app-$how.log" | grep -v -i -E 'AT-SPI|dbind|libEGL|portal|pw\.conf|pipewire|dri3|mesa' >/dev/null; then
    fail "[$how] errors in the app log:"
    grep -i -E 'panic|error' "$SANDBOX/app-$how.log" | head -5
  else
    pass "[$how] no errors in the app log"
  fi
}

# One D-Bus session with an unlocked throwaway keyring for all cycles.
eval "$(dbus-launch --sh-syntax)"
echo -n "smoke-only-password" | gnome-keyring-daemon --unlock --components=secrets >/dev/null
for how in close term int; do
  cycle "$how"
done
kill "$DBUS_SESSION_BUS_PID" 2>/dev/null || true

# The app may write only its settings file; WebKit, GTK and Mesa caches are
# not the app's (and are absent on a real desktop where they already exist).
OWN="$(find "$HOME" -type f \
  -not -path '*/mesa_shader_cache*' -not -path '*/dconf/*' -not -path '*/keyrings/*' \
  -not -path '*/.cache/doc/*' -not -path '*/openbox/*' 2>/dev/null || true)"
if [ "$OWN" = "$XDG_CONFIG_HOME/io.github.alveeerahman.mr-usage/settings.json" ] || [ -z "$OWN" ]; then
  pass "app wrote nothing to disk except its settings file"
else
  fail "unexpected files written:"
  echo "$OWN"
fi
if [ -f "$XDG_CONFIG_HOME/io.github.alveeerahman.mr-usage/settings.json" ] \
  && grep -q -i -E 'key|token|email|remaining' "$XDG_CONFIG_HOME/io.github.alveeerahman.mr-usage/settings.json"; then
  fail "settings file holds a secret-looking field"
fi

case "$KIND" in
  deb) $SUDO apt-get purge -y mr-usage >/dev/null ;;
  rpm) $SUDO dnf remove -y mr-usage >/dev/null ;;
  appimage) rm -f "$PKG" ;;
esac
LEFT=0
for f in "${FILES[@]}"; do
  if [ -e "$f" ]; then
    echo "  left behind: $f"
    LEFT=$((LEFT + 1))
  fi
done
[ "$LEFT" = "0" ] && pass "uninstall removed all ${#FILES[@]} installed file(s)" || fail "uninstall left $LEFT file(s)"
if [ "$KIND" = deb ]; then
  dpkg -s mr-usage >/dev/null 2>&1 && fail "package still registered with dpkg" || pass "package no longer registered with dpkg"
fi

echo "SMOKE $KIND: $FAILS failure(s)"
[ "$FAILS" = "0" ]
