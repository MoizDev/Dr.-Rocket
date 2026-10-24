#!/bin/bash
# Dev loop: run Dr. Rocket, rebuild and restart it whenever the sources change.
# Theme JSON edits (assets/themes) apply live without a restart.
#   tools/dev.sh [file-or-folder]
#   DRROCKET_DEV_PROFILE=devfast tools/dev.sh  # release-speed code, slower first build
set -u
cd "$(dirname "$0")/.." || exit 1
ROOT="$PWD"
PROFILE="${DRROCKET_DEV_PROFILE:-dev}"
case "$PROFILE" in
  dev) BIN="$ROOT/target/debug/drrocket" ;;
  devfast) BIN="$ROOT/target/devfast/drrocket" ;;
  *) echo "dev: DRROCKET_DEV_PROFILE must be dev or devfast" >&2; exit 1 ;;
esac
export CARGO_TARGET_DIR="$ROOT/target"
export DRROCKET_CONFIG_DIR="$ROOT/target/dev-profile"
export DRROCKET_CACHE_DIR="$DRROCKET_CONFIG_DIR/cache"
mkdir -p "$DRROCKET_CONFIG_DIR" || exit 1
if [ ! -e "$DRROCKET_CONFIG_DIR/settings.json" ]; then
  printf '%s\n' '{"check_for_updates":false}' > "$DRROCKET_CONFIG_DIR/settings.json" || exit 1
fi
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
trap 'stop; rm -f "$STAMP"' EXIT
trap 'exit 0' INT TERM

cargo build --locked --profile "$PROFILE" || exit 1
touch "$STAMP"
start "$@"
echo "dev: running (pid $PID). Watching src/, racket/, assets/ (themes reload live)."
echo "dev: settings and caches are in $DRROCKET_CONFIG_DIR. Save edits before a rebuild; close the app or press Ctrl+C to stop."
while true; do
  sleep 0.4
  kill -0 "$PID" 2>/dev/null || exit 0
  changed=$(find src racket assets/SVG assets/icons assets/icon assets/fonts Cargo.toml Cargo.lock build.rs -newer "$STAMP" -type f 2>/dev/null | head -1)
  [ -z "$changed" ] && continue
  touch "$STAMP"
  echo "dev: $changed changed, rebuilding..."
  if cargo build --locked --profile "$PROFILE"; then
    stop
    start "$@"
    echo "dev: restarted (pid $PID)"
  else
    echo "dev: build failed, keeping the running app"
  fi
done
