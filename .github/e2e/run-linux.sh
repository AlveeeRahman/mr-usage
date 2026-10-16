#!/usr/bin/env bash
# Build the app and run .github/e2e/drive.py against it under a virtual display with
# a throwaway GNOME Keyring. Used by CI; runnable locally on Linux.
# Needs: xvfb, gnome-keyring, dbus, libsecret-tools, webkit2gtk-driver,
# tauri-driver (cargo install tauri-driver --version 2.1.0 --locked), and
# the Tauri CLI (cargo install tauri-cli --version 2.12.3 --locked).
set -euo pipefail
cd "$(dirname "$0")/../.."
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

# Every failure path below ends in `fail`, which also repeats the reason, the
# driver's output and the tail of the app log as GitHub annotations so they
# show in the run summary.
annotate() {
  local title="$1" text="$2"
  text="${text//'%'/%25}"
  text="${text//$'\r'/%0D}"
  text="${text//$'\n'/%0A}"
  echo "::error title=${title}::${text}"
}
fail() {
  echo "FAIL: $1"
  if [ -n "${GITHUB_ACTIONS:-}" ]; then
    annotate "e2e failed" "$1"
    annotate "e2e driver output (last 40 lines)" "$(tail -n 40 "$OUT/drive.log" 2>/dev/null || echo "no drive.log")"
    annotate "e2e app log (last 60 lines)" "$(tail -n 60 "$OUT/app.log" 2>/dev/null || echo "no app.log")"
  fi
  exit 1
}

DRIVE_RC=0
dbus-run-session -- bash -c '
  set -uo pipefail
  echo -n "e2e-only-password" | gnome-keyring-daemon --unlock --components=secrets >/dev/null || exit 70
  MR_USAGE_LOG=debug tauri-driver --native-driver "$(command -v WebKitWebDriver)" >"'"$OUT"'/app.log" 2>&1 &
  DRIVER=$!
  # tauri-driver takes a moment to listen; a cold runner takes longer than a warm laptop.
  for _ in $(seq 1 60); do
    curl -fsS http://127.0.0.1:4444/status >/dev/null 2>&1 && break
    sleep 0.5
  done
  python3 -I .github/e2e/drive.py "'"$OUT"'" "'"$APP"'" >"'"$OUT"'/drive.log" 2>&1
  rc=$?
  cat "'"$OUT"'/drive.log"
  kill $DRIVER 2>/dev/null || true
  exit $rc
' || DRIVE_RC=$?
[ "$DRIVE_RC" = 70 ] && fail "gnome-keyring-daemon did not start or unlock"
[ "$DRIVE_RC" = 0 ] || fail "drive.py exited with code $DRIVE_RC"

FAILED="$(python3 -I - "$OUT/results.json" <<'PY' || true
import json, sys
try:
    results = json.load(open(sys.argv[1]))
except Exception as e:
    print(f"results.json unreadable: {e}")
    sys.exit(0)
print("\n".join(f"{name}: {detail}" for name, ok, detail in results if not ok) or ("no results" if not results else ""))
PY
)"
[ -z "$FAILED" ] || fail "checks failed: $FAILED"

KEY="e2e0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0"
# The leak grep is only meaningful if the app's own log reached app.log. The
# fake key always fails its first poll, which logs "poll failed" at warn.
grep -q "poll failed" "$OUT/app.log" || fail "app.log holds no log line from the app, so the leak grep would prove nothing"
for needle in "$KEY" api_key Authorization; do
  grep -q "$needle" "$OUT/app.log" && fail "app log contains $needle"
done
[ -f "$SETTINGS" ] || fail "settings file was not written"
grep -q -i -e "$KEY" -e key -e email "$SETTINGS" && fail "settings file contains the key"

echo "E2E PASSED (artifacts in $OUT)"
