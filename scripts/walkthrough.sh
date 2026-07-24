#!/usr/bin/env bash
# Regenerate the guide screenshots by driving the app's walkthrough mode
# (MANDELBROT_WALKTHROUGH) under Xvfb and grabbing a frame per
# "WALKTHROUGH-SHOT <name>" marker. Output: docs/guide/<name>.png
#
# Usage: scripts/walkthrough.sh <app-command...>
# Env:
#   WALKTHROUGH_DISPLAY  reuse this X display instead of starting Xvfb (:91)
#   WALKTHROUGH_ENV      env file from scripts/walkthrough-seed.sh; when set,
#                        a second, logged-in pass runs with those credentials
#   WALKTHROUGH_SCHEME   `default`, `dark` or `light` (ADW_DEBUG_COLOR_SCHEME)
#   WALKTHROUGH_SUFFIX   appended to every file name (used for the dark pass)
#
# Dependencies: Xvfb, xdpyinfo, xwd, netpbm (xwdtopnm/pnmtopng). netpbm is
# used rather than ImageMagick because ImageMagick's xwd delegate is disabled
# in most distro policy files.
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: walkthrough.sh <app-command...>" >&2; exit 2; }
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=$ROOT/docs/guide
SUFFIX=${WALKTHROUGH_SUFFIX:-}
mkdir -p "$OUT"

DISPLAY_NUM=${WALKTHROUGH_DISPLAY:-:91}
if ! xdpyinfo -display "$DISPLAY_NUM" > /dev/null 2>&1; then
  Xvfb "$DISPLAY_NUM" -screen 0 1400x950x24 > /dev/null 2>&1 &
  XVFB_PID=$!
  trap 'kill $XVFB_PID 2>/dev/null || true' EXIT
  sleep 2
fi

capture() { # capture <name>
  DISPLAY=$DISPLAY_NUM xwd -root -silent > /tmp/wt-frame.xwd || true
  if [ -s /tmp/wt-frame.xwd ]; then
    xwdtopnm < /tmp/wt-frame.xwd 2> /dev/null | pnmtopng > "$OUT/$1$SUFFIX.png"
    echo "captured $1$SUFFIX"
  else
    echo "empty frame for $1" >&2
  fi
}

run_pass() { # run_pass <extra-env...> -- runs the app, captures per marker
  env GDK_BACKEND=x11 WAYLAND_DISPLAY= DISPLAY=$DISPLAY_NUM \
    GSK_RENDERER=cairo \
    ADW_DEBUG_COLOR_SCHEME=${WALKTHROUGH_SCHEME:-default} \
    MANDELBROT_WALKTHROUGH=1 "$@" 2> /dev/null |
    while read -r line; do
      case "$line" in
        "WALKTHROUGH-SHOT "*) capture "${line#WALKTHROUGH-SHOT }" ;;
      esac
    done
}

# Pass 1: everything reachable without a Matrix session.
run_pass "$@"

# Pass 2: the feature UI of a logged-in session seeded by walkthrough-seed.sh.
if [ -n "${WALKTHROUGH_ENV:-}" ] && [ -f "$WALKTHROUGH_ENV" ]; then
  # `env` takes the NAME=VALUE assignments before the command, so the seed
  # file can be handed to run_pass verbatim.
  mapfile -t SEED < <(grep -E '^[A-Z_]+=' "$WALKTHROUGH_ENV")
  run_pass "${SEED[@]}" "$@"
fi

"$ROOT/scripts/walkthrough-validate.py" "$OUT"
