#!/usr/bin/env bash
# Launch the app inside a flatpak build sandbox for the screenshot walkthrough.
#
# `flatpak build` does NOT inherit the caller's environment, so every variable
# the walkthrough needs has to be forwarded explicitly with `--env=`. This
# wrapper does that for `MANDELBROT_WALKTHROUGH*` plus the display/rendering
# variables, and is what scripts/walkthrough.sh should be pointed at when the
# app lives in a flatpak build directory rather than on `$PATH`.
#
# Usage: scripts/walkthrough-flatpak-run.sh <flatpak-build-dir> [app-path]
set -euo pipefail
BUILD_DIR=${1:?usage: walkthrough-flatpak-run.sh <flatpak-build-dir> [app-path]}
APP=${2:-/app/bin/fractal}

args=(--share=network --socket=x11 --filesystem=home)
args+=("--env=DISPLAY=${DISPLAY:-:91}")
args+=(--env=GDK_BACKEND=x11)
# llvmpipe under Xvfb otherwise renders blank frames.
args+=(--env=GSK_RENDERER=cairo)
args+=("--env=ADW_DEBUG_COLOR_SCHEME=${ADW_DEBUG_COLOR_SCHEME:-default}")
args+=("--env=RUST_LOG=${RUST_LOG:-fractal=info,warn}")
# A build directory that was only `meson install`ed partially may be missing
# the compiled GSettings schema, which the app aborts without.
[ -n "${GSETTINGS_SCHEMA_DIR:-}" ] && args+=("--env=GSETTINGS_SCHEMA_DIR=$GSETTINGS_SCHEMA_DIR")

while IFS='=' read -r name _; do
  case "$name" in
    MANDELBROT_WALKTHROUGH*) args+=("--env=$name=${!name}") ;;
  esac
done < <(env)

exec flatpak build "${args[@]}" "$BUILD_DIR" "$APP"
