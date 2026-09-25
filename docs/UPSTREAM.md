# Upstream Synchronization and Tracking

Mandelbrot is a downstream fork of [GNOME Fractal](https://gitlab.gnome.org/World/fractal).
This document tracks the upstream baseline, describes divergence boundaries, and outlines the
process for synchronizing upstream changes and security fixes.

## Baseline

* **Upstream Repository**: `https://gitlab.gnome.org/World/fractal.git`
* **Upstream License**: GPL-3.0-or-later
* **Initial Fork Baseline Commit**: `a262c7d656cc4c3a87656370e10cd279cf6e081a`
* **Initial Fork Baseline Tag**: Fractal 14.1 (released July 19, 2026)
* **First Mandelbrot Tag**: v14.1.1 (released July 24, 2026)

## Divergence Boundaries

Mandelbrot maintains full feature parity with Fractal while developing downstream extensions:

1. **Native MatrixRTC Calling (`matrixrtc/` crate & `src/session/room/call.rs`)**:
   * MatrixRTC session state machine and MSC4140 delayed event handling.
   * LiveKit Rust SDK media pipeline and GStreamer camera/microphone capture.
   * Native GTK4/Adwaita calling UI shell and freedesktop notification portal v2 integration.

2. **Simplified Sliding Sync (MSC4186)**:
   * Uses `matrix-sdk-ui` `SyncService` and `RoomListService` with fallback to classic sync.

3. **QR Code Login (MSC4108)**:
   * Bi-directional OAuth QR scanning and grantor device link flows.

4. **Timeline Enhancements**:
   * MSC3440 thread support with adaptive side pane.
   * MSC3381 poll creation and voting.
   * MSC3245 voice message recording.
   * Spaces hierarchy browsing.

5. **Packaging and Infrastructure**:
   * GHCR OCI Flatpak packaging (`org.tunaos.mandelbrot.json`).
   * Upstream sync automation script (`build-aux/sync-upstream.sh`).

## Synchronization Process

### Synchronization Script

The sync script is located at `build-aux/sync-upstream.sh`.

To run upstream synchronization:

```sh
./build-aux/sync-upstream.sh [upstream_branch]
```

It performs the following steps:

1. Fetches latest branches and tags from `https://gitlab.gnome.org/World/fractal.git`.
2. Computes commits introduced in upstream `main` since the recorded baseline.
3. Prepares a new synchronization branch (`sync/upstream-YYYYMMDD`).
4. Attempts an automated merge against Mandelbrot `main` and reports status or merge conflicts.

### Conflict Resolution Strategy

When resolving conflicts during an upstream sync:

* **Core Matrix Features**: Prefer upstream fixes for common matrix-sdk interactions and protocol
  parsers unless they conflict with Mandelbrot's sliding sync / MatrixRTC integrations.
* **MatrixRTC / Calls**: Mandelbrot's `matrixrtc/` crate and calling components are downstream
  additions; ensure changes to `src/session/` preserve call manager hooks.
* **Crate Naming**: Keep `Cargo.toml` and `meson.build` crate metadata aligned with upstream
  tracking until a scheduled tree-wide rename.
* **Repository Governance Files**: `AGENTS.md` states this fork's own contributor-automation
  contract and must not be replaced wholesale by an upstream merge. If a sync brings in an
  upstream change to that file (or to `CONTRIBUTING.md`'s policy sections), reconcile it —
  keep upstream's substantive policy changes, but do not let a merge silently reinstate
  content this fork has deliberately overridden. This is the gap that let
  upstream's "Instructions for LLM agents" file (a satirical policy-against-AI statement,
  see `CONTRIBUTING.md`'s "LLM Contributions" section for the real policy it stood in for)
  sit unreconciled in `AGENTS.md` for weeks after this fork started running the App-authored
  maintenance the old file's own text told every agent not to do.
