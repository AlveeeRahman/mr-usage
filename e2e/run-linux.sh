#!/usr/bin/env bash
# Build the app and run e2e/drive.py against it under a virtual display with
# a throwaway GNOME Keyring. Used by CI; runnable locally on Linux.
# Needs: xvfb, gnome-keyring, dbus, libsecret-tools, webkit2gtk-driver,
# tauri-driver (cargo install tauri-driver --version 2.1.0 --locked), and
# the Tauri CLI (cargo install tauri-cli --version 2.12.3 --locked).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-$(mktemp -d)}"
mkdir -p "$OUT"

cargo tauri build --debug --no-bundle
APP="$PWD/target/debug/mr-usage"

# A throwaway keyring and config directory, so a run never depends on (or
# touches) the user's real ones.
SANDBOX="$(mktemp -d)"
export XDG_DATA_HOME="$SANDBOX/data" XDG_CONFIG_HOME="$SANDBOX/config"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME"
SETTINGS="$XDG_CONFIG_HOME/io.github.alveeerahman.mr-usage/settings.json"

export DISPLAY=:99
Xvfb :99 -screen 0 1280x900x24 >/dev/null 2>&1 &
XVFB=$!
trap 'kill $XVFB 2>/dev/null || true; rm -rf "$SANDBOX"' EXIT

dbus-run-session -- bash -c '
  set -euo pipefail
  echo -n "e2e-only-password" | gnome-keyring-daemon --unlock --components=secrets >/dev/null
  MR_USAGE_LOG=debug tauri-driver --native-driver "$(command -v WebKitWebDriver)" >"'"$OUT"'/app.log" 2>&1 &
  DRIVER=$!
  sleep 2
  python3 -I e2e/drive.py "'"$OUT"'" "'"$APP"'"
  kill $DRIVER 2>/dev/null || true
'

python3 -I - "$OUT/results.json" <<'PY'
import json, sys
results = json.load(open(sys.argv[1]))
failed = [name for name, ok, _ in results if not ok]
sys.exit(1 if failed or not results else 0)
PY

KEY="e2e0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0"
for needle in "$KEY" api_key Authorization; do
  if grep -q "$needle" "$OUT/app.log"; then
    echo "FAIL: app log contains $needle"
    exit 1
  fi
done
if [ ! -f "$SETTINGS" ]; then
  echo "FAIL: settings file was not written"
  exit 1
fi
if grep -q -i -e "$KEY" -e key -e email "$SETTINGS"; then
  echo "FAIL: settings file contains the key"
  exit 1
fi
echo "E2E PASSED (artifacts in $OUT)"
