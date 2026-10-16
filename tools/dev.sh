#!/bin/bash
# Dev loop: run Dr. Rocket, rebuild and restart it whenever the sources change.
# Theme JSON edits (assets/themes) apply live without a restart.
#   tools/dev.sh [file-or-folder]
set -u
cd "$(dirname "$0")/.."
ROOT="$PWD"
BIN="$ROOT/target/debug/drrocket"
STAMP="$(mktemp -t drrocket-dev)"
PID=""

start() {
  DRROCKET_DEV="$ROOT" RUST_BACKTRACE=1 "$BIN" "$@" &
  PID=$!
}
stop() {
  [ -n "$PID" ] && kill "$PID" 2>/dev/null && wait "$PID" 2>/dev/null
  PID=""
}
trap 'stop; rm -f "$STAMP"; exit 0' INT TERM

cargo build || exit 1
touch "$STAMP"
start "$@"
echo "dev: running (pid $PID). Watching src/, racket/, assets/ (themes reload live)."
while true; do
  sleep 0.4
  changed=$(find src racket assets/SVG assets/icons assets/icon assets/fonts Cargo.toml build.rs -newer "$STAMP" -type f 2>/dev/null | head -1)
  [ -z "$changed" ] && continue
  touch "$STAMP"
  echo "dev: $changed changed, rebuilding..."
  if cargo build; then
    stop
    start "$@"
    echo "dev: restarted (pid $PID)"
  else
    echo "dev: build failed, keeping the running app"
  fi
done
