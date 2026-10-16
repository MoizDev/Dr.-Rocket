#!/bin/sh
# Run Dr. Rocket under an automation script and wait for it to exit.
# Usage: tools/shot.sh script.json [config-dir]
set -e
script="$1"
cfg="${2:-$(mktemp -d)}"
bin="$(dirname "$0")/../target/debug/racketeer"
[ -x "$(dirname "$0")/../target/debug/drrocket" ] && bin="$(dirname "$0")/../target/debug/drrocket"
RACKETEER_CONFIG_DIR="$cfg" DRROCKET_CONFIG_DIR="$cfg" DRROCKET_AUTOMATION="$script" "$bin" &
pid=$!
i=0
while kill -0 $pid 2>/dev/null; do
  i=$((i + 1))
  if [ $i -gt 600 ]; then kill $pid; echo "timeout"; exit 1; fi
  sleep 0.2
done
