#!/bin/sh
# Measures the real app against an invented vault, beside whatever else is running.
#
#   CARGO_PROFILE_DEV_OPT_LEVEL=3 bun run tauri build --debug --no-bundle
#   cargo run --release -p bm-core --example seed_vault -- <dir> perf-messung-1234 50000
#   app/scripts/perf-app.sh <dir> <results file>
#
# Copy the model into <dir>/models to measure indexing and search by meaning as well.
# Opens a window: covering it stops the frames, so leave it visible while this runs.
# BM_PERF_RETINA=1 puts it on a Retina display, if there is one. To measure for longer than
# the vault stays unlocked, turn off locking in <dir>/settings.json first.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
vault=$1
out=$2
limit=${3:-1800}

memory() {
  # Everything the web view does happens in processes of its own.
  web=0
  for pid in $(pgrep -f com.apple.WebKit.WebContent || true); do
    case " $before " in *" $pid "*) ;; *)
      kb=$(ps -o rss= -p "$pid" 2>/dev/null || echo 0)
      web=$((web + kb))
    ;; esac
  done
  app=$(ps -o rss= -p "$bin_pid" 2>/dev/null || echo 0)
  echo "{\"name\":\"memory\",\"appMb\":$((app / 1024)),\"webMb\":$((web / 1024)),\"at\":$(perl -MTime::HiRes=time -e 'printf "%d", time()*1000')}" >>"$out"
}

before=$(pgrep -f com.apple.WebKit.WebContent | tr '\n' ' ' || true)
: >"$out"
echo "{\"name\":\"launch\",\"at\":$(perl -MTime::HiRes=time -e 'printf "%d", time()*1000')}" >>"$out"

# PERF_DPR=2 measures the load of a Retina display on one that is not.
script="$out.js"
{ echo "window.__bmPerfDpr = ${PERF_DPR:-0};"; cat "$root/app/scripts/perf-app.js"; } >"$script"

BM_PERF_SCRIPT="$script" BM_PERF_VAULT="$vault" BM_PERF_OUT="$out" \
  RUST_LOG=info "$root/target/debug/brainmatter" >"$out.log" 2>&1 &
bin_pid=$!
trap 'kill "$bin_pid" 2>/dev/null || true' EXIT

# Optional: the path of a helper that prints the window id of a process, for a look at the result.
shot_done=
shoot() {
  [ -n "${PERF_WINDOW_ID:-}" ] && [ -z "$shot_done" ] && grep -q '"name":"settled"' "$out" || return 0
  shot_done=1
  sleep 1
  id=$("$PERF_WINDOW_ID" "$bin_pid")
  [ "$id" != 0 ] && screencapture -x -o -l "$id" "$out.png" || true
}

waited=0
while [ "$waited" -lt "$limit" ] && ! grep -qE '"name":"(done|failed)"' "$out"; do
  sleep 5
  waited=$((waited + 5))
  shoot
  memory
done
memory
