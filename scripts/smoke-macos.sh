#!/usr/bin/env bash
# Install, start, stop and uninstall smoke test for the macOS disk image.
#
#   scripts/smoke-macos.sh <path/to/Mr. Usage_*.dmg>
#
# Checks: the DMG mounts and holds the app; copying it to /Applications
# installs it; it launches through LaunchServices (`open`) and directly; it
# stops cleanly on SIGTERM and SIGINT (exit code 0 within 10 s); it starts
# again after each stop; deleting the bundle removes it. Per-user data left
# afterwards is reported, not failed (README, "Uninstalling").
set -euo pipefail

DMG="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
APP="/Applications/Mr. Usage.app"
BIN="$APP/Contents/MacOS/mr-usage"
ID="io.github.alveeerahman.mr-usage"
FAILS=0
pass() { echo "PASS $*"; }
fail() { echo "FAIL $*"; FAILS=$((FAILS + 1)); }

MNT="$(mktemp -d)"
hdiutil attach -nobrowse -readonly -mountpoint "$MNT" "$DMG" >/dev/null
[ -d "$MNT/Mr. Usage.app" ] && pass "DMG mounts and contains Mr. Usage.app" || fail "app not found in DMG"
rm -rf "$APP"
cp -R "$MNT/Mr. Usage.app" /Applications/
hdiutil detach "$MNT" >/dev/null
[ -x "$BIN" ] && pass "installed to $APP" || { fail "binary missing at $BIN"; exit 1; }
codesign --verify "$APP" 2>/dev/null && pass "bundle signature verifies (ad-hoc)" || echo "INFO bundle is unsigned or ad-hoc signed only (see README)"

wait_exit() { # pid -> sets CODE
  local pid=$1
  for _ in $(seq 1 100); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
  if kill -0 "$pid" 2>/dev/null; then
    fail "still running 10 s after the stop request"
    kill -KILL "$pid" 2>/dev/null || true
  fi
}

# Direct launch: we own the PID and can read the exit code.
for sig in TERM INT; do
  "$BIN" >/tmp/mr-usage-smoke.log 2>&1 &
  pid=$!
  sleep 8
  if ! kill -0 "$pid" 2>/dev/null; then
    fail "[$sig] app exited on its own during startup"; cat /tmp/mr-usage-smoke.log; continue
  fi
  pass "[$sig] started (pid $pid)"
  kill "-$sig" "$pid"
  wait_exit "$pid"
  code=0; wait "$pid" || code=$?
  [ "$code" = "0" ] && pass "[$sig] exited with code 0" || fail "[$sig] exit code $code"
  if grep -i -E 'panic|error' /tmp/mr-usage-smoke.log >/dev/null; then fail "[$sig] errors in app log"; grep -i -E 'panic|error' /tmp/mr-usage-smoke.log | head -5; fi
done

# LaunchServices launch, as a user double-clicking the app would.
open -n "$APP"
pid=""
for _ in $(seq 1 30); do pid="$(pgrep -x mr-usage | head -n1 || true)"; [ -n "$pid" ] && break; sleep 0.5; done
if [ -n "$pid" ]; then
  sleep 5
  pass "[open] started through LaunchServices (pid $pid)"
  kill -TERM "$pid"
  wait_exit "$pid"
  pgrep -x mr-usage >/dev/null && fail "[open] process still present" || pass "[open] stopped cleanly"
else
  fail "[open] did not start"
fi
sleep 2
pgrep -x mr-usage >/dev/null && fail "mr-usage processes left running" || pass "no mr-usage processes left"

rm -rf "$APP"
[ ! -e "$APP" ] && pass "uninstall (delete bundle) removed the app" || fail "app still present"

for d in "$HOME/Library/Application Support/$ID" "$HOME/Library/Caches/$ID" "$HOME/Library/WebKit/$ID" "$HOME/Library/WebKit/mr-usage" "$HOME/Library/Saved Application State/$ID.savedState"; do
  if [ -e "$d" ]; then
    echo "INFO user data present after uninstall: $d"
    find "$d" -maxdepth 3 | head -10 | sed 's/^/    /'
  fi
done
S="$HOME/Library/Application Support/$ID/settings.json"
if [ -f "$S" ] && grep -q -i -E 'key|token|email|remaining' "$S"; then fail "settings.json holds a secret-looking field"; fi

echo "SMOKE macos: $FAILS failure(s)"
[ "$FAILS" = "0" ]
