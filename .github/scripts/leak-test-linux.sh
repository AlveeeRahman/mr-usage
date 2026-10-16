#!/usr/bin/env bash
# Core-dump leak test for the keystore wrapper, adapted from keyring-rs's
# examples/leak-test/run-linux-test.bash (read 2026-10-16). Upstream uses
# Sysinternals procdump; this uses gdb's gcore, which most distros package.
#
# Needs: a running, unlocked Secret Service (GNOME Keyring, KWallet, ...),
# gdb, and a ptrace policy that lets you dump your own processes
# (kernel.yama.ptrace_scope <= 1 when run as the same user).
#
# Exit code 0: the secret was not found in the dump. 1: it was, or the test
# could not run.
set -euo pipefail

cd "$(dirname "$0")/../.."
DUMP_DIR="$(mktemp -d)"
trap 'rm -rf "$DUMP_DIR"' EXIT

echo "Building..."
cargo build --quiet -p mr-usage-core --example leak_test
BIN="target/debug/examples/leak_test"

echo "Running..."
DELAY_SECS=6 "$BIN" &
PID=$!
sleep 3

echo "Dumping..."
if ! gcore -o "$DUMP_DIR/leak" "$PID" >/dev/null 2>&1; then
  echo "TEST ABORT - gcore failed"
  wait "$PID" || true
  exit 1
fi
wait "$PID"

DUMP="$(ls "$DUMP_DIR"/leak.* | head -n1)"
# Extract once to a file: `grep -q` in a pipe would SIGPIPE `strings`, and
# pipefail would turn a match into a failure.
strings -a "$DUMP" > "$DUMP_DIR/strings.txt"
MARKER="$(grep -c "ready-for-a-close-up" "$DUMP_DIR/strings.txt" || true)"
if [ "$MARKER" = "0" ]; then
  echo "TEST ABORT - marker string not found, so the scan itself is broken"
  exit 1
fi

HITS="$(grep -c "super-duper-password" "$DUMP_DIR/strings.txt" || true)"
if [ "$HITS" != "0" ]; then
  echo "TEST FAILED - secret found $HITS time(s) in the process dump"
  exit 1
fi
echo "TEST PASSED - secret not found in the process dump"
