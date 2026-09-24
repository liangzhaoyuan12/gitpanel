#!/usr/bin/env bash
# Startup latency + RSS baseline for gitpanel (GOAL 0.2).
#
# Measures time from exec until the main window appears on X11 (xdotool
# window search), plus resident memory shortly after. Takes the median of N
# runs. Usage: scripts/startup-bench.sh [runs] [repo_path]
set -euo pipefail

RUNS="${1:-5}"
REPO="${2:-$(cd "$(dirname "$0")/.." && pwd)}"
BIN="$REPO/target/release/gitpanel"

[ -x "$BIN" ] || { echo "missing $BIN — run: cargo build --release" >&2; exit 1; }
[ -n "${DISPLAY:-}" ] || { echo "no DISPLAY" >&2; exit 1; }

pids=()
declare -a TIMES=()
declare -a RSSS=()

cleanup() {
    for p in "${pids[@]:-}"; do kill "$p" 2>/dev/null || true; done
    wait 2>/dev/null || true
}
trap cleanup EXIT

for i in $(seq "$RUNS"); do
    start=$(date +%s.%N)
    "$BIN" "$REPO" >/dev/null 2>&1 &
    app_pid=$!
    pids+=("$app_pid")

    # Poll for the main window (max 10s).
    win=""
    deadline=$((SECONDS + 10))
    while [ -z "$win" ] && [ "$SECONDS" -lt "$deadline" ]; do
        win=$(xdotool search --pid "$app_pid" --onlyvisible 2>/dev/null | head -1 || true)
        [ -z "$win" ] && sleep 0.02
    done
    end=$(date +%s.%N)
    if [ -z "$win" ]; then
        echo "run $i: window did not appear within 10s" >&2
        continue
    fi

    # RSS once the window exists (VmRSS in KiB).
    rss=$(awk '/VmRSS/{print $2}' "/proc/$app_pid/status" 2>/dev/null || echo 0)
    TIMES+=("$(awk -v a="$end" -v b="$start" 'BEGIN{printf "%.0f", (a-b)*1000}')")
    RSSS+=("$rss")

    # Give the app a moment, then close it before the next run.
    sleep 0.5
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
done

[ "${#TIMES[@]}" -gt 0 ] || { echo "no successful runs" >&2; exit 1; }

median() {
    printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'
}

echo "runs:   ${TIMES[*]} ms"
echo "median: $(median "${TIMES[@]}") ms  (window visible)"
echo "median RSS: $(median "${RSSS[@]}") KiB"
