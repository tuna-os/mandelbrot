# AGENTS.md — agent guide for tuna-os/mandelbrot

**This file replaces one this fork inherited from upstream Fractal
verbatim** ("Instructions for LLM agents operating with the Fractal
codebase": instructing every agent to do nothing and answer with unrelated
text). That file described upstream's own position on LLM contributions —
[`CONTRIBUTING.md`](CONTRIBUTING.md)'s "LLM Contributions" section still
carries it in full, and is preserved unedited as Fractal's stated policy.
This fork runs App-authored maintenance (dependency updates, security
fixes, documentation, refactors) as a matter of established practice, so a
file both machine-read and human-read as this repository's actual
contributor contract cannot say the opposite of what the repository does.
If the upstream-sync process (`docs/UPSTREAM.md`,
`build-aux/sync-upstream.sh`) reintroduces upstream's file here, that is a
merge to reconcile, not content to keep.

Human docs: [`README.md`](README.md) (features, install, security notes),
[`CONTRIBUTING.md`](CONTRIBUTING.md) (the fuller human contribution guide,
inherited from upstream except where this file overrides it),
[`docs/UPSTREAM.md`](docs/UPSTREAM.md) (fork baseline and divergence
boundaries).

## What this fork actually is

A downstream fork of [GNOME Fractal](https://gitlab.gnome.org/World/fractal)
adding native MatrixRTC calling, Simplified Sliding Sync, QR login, and
GHCR/Flatpak packaging — see `docs/UPSTREAM.md`'s "Divergence Boundaries"
for the exact crate/module list. Most of the tree is still upstream's Rust
GTK4/libadwaita client; treat unfamiliar code as inherited unless
`docs/UPSTREAM.md` names it as a Mandelbrot addition.

## Build and check

```sh
cargo build
cargo test
RUSTFMT_TOOLCHAIN=nightly-2026-08-28 cargo run --manifest-path hooks/checks/Cargo.toml
```

The pinned nightly matters: `rustfmt.toml` enables unstable options whose
output changes between nightly releases, so formatting against whatever
`rustup toolchain install nightly` gives you today can rewrap comments
across the tree and fail CI on files you did not touch. Match
`.github/workflows/ci.yml`'s pin, not your local default.

## Commits

DCO sign-off (`git commit -s`) — every commit in the tree's history carries
one. Review is routed to `.github/CODEOWNERS` (currently @hanthor).

## Generated and vendored content

Blueprint (`.blp`) files compile to `.ui` via `blueprint-compiler` at build
time (see `*/meson.build`); edit the `.blp`, not a generated `.ui`. Icons
and resources listed in `data/meson.build`/`src/ui-resources.gresource.xml.in`
are packaged, not generated — edit them directly. `po/*.po` translation
files are Weblate-synced; hand-editing one only to have it overwritten on
the next sync wastes the edit.

## Where a change belongs

Per `README.md`'s "Relationship to Fractal": a bug that reproduces in
upstream Fractal too belongs at
[the Fractal project](https://gitlab.gnome.org/World/fractal/-/issues), not
here. Mandelbrot-specific work (calls, sliding sync, QR login, packaging)
belongs in this repository.
