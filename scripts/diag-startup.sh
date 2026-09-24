#!/usr/bin/env bash
# one-shot startup diagnostic: launch, inspect window matching, kill
cd "$(dirname "$0")/.."
target/release/gitpanel . > /tmp/gp.log 2>&1 &
APP=$!
sleep 3
if ps -p "$APP" >/dev/null 2>&1; then echo "ALIVE pid=$APP"; else echo "DEAD"; fi
echo "--- log:"; head -10 /tmp/gp.log
echo "--- search --pid:"; xdotool search --pid "$APP" 2>&1 | head -5
echo "--- visible windows:"
for w in $(xdotool search --onlyvisible --name "." 2>/dev/null); do
  echo "  $w: $(xdotool getwindowname "$w" 2>/dev/null)"
done
echo "--- by class:"
for w in $(xdotool search --class gitpanel 2>/dev/null); do
  echo "  $w: $(xdotool getwindowname "$w" 2>/dev/null) pid=$(xdotool getwindowpid "$w" 2>/dev/null)"
done
kill "$APP" 2>/dev/null
wait "$APP" 2>/dev/null
echo DONE
