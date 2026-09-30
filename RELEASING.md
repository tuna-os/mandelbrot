# Releasing Mandelbrot

This describes how this fork actually ships, which differs from upstream
Fractal: there is no GitLab remote, and publishing does not go through a
Flathub pull request. Releases are OCI Flatpaks pushed straight to GHCR by
a GitHub Actions workflow when a `v*` tag lands.

## Before making a new release

* Update dependencies (crates or system libraries) and migrate from deprecated APIs.
* Make sure `org.tunaos.mandelbrot.json` targets the runtime version this release should ship
  against (currently GNOME `50`, see `runtime-version` in that file).

## Making a release

1. Bump the version:
   * `/meson.build`: update `major_version` (and `pre_release_version` for a beta/pre-release;
     leave it empty for stable).
   * `/Cargo.toml`: update `version` to match, using semver.
2. Update `/data/org.tunaos.mandelbrot.metainfo.xml.in.in`: add a new `release` entry at the top
   of `releases` with the new version and today's date.
3. Commit these version-bump changes, get them merged to `main` via the normal PR process
   (`CONTRIBUTING.md`).
4. Tag the merge commit and push the tag:

   ```sh
   git tag vMAJOR.MINOR.PATCH
   git push origin vMAJOR.MINOR.PATCH
   ```

   The tag must start with `v` — that is what `.github/workflows/publish-flatpak.yml` triggers
   on.
5. Pushing the tag triggers `publish-flatpak.yml`, which calls this org's reusable
   `tuna-os/.github/.github/workflows/publish-flatpak.yml@main` to build the Flatpak from
   `org.tunaos.mandelbrot.json` and publish it as an OCI image to GHCR
   (`ghcr.io/tuna-os/mandelbrot`), tagged for the `tuna-os` Flatpak remote described in
   `README.md`'s Installation section.
6. If the build needs to be re-run without a new tag (for example, after fixing a transient CI
   failure), the workflow also accepts manual dispatch: on GitHub, open **Actions → Build and
   Publish Flatpak OCI → Run workflow**.

There is currently **no GitHub Releases page** for this repository — no binaries, checksums, or
changelog on the Releases surface, only the OCI image on GHCR. Publishing a Releases-page artifact
alongside the OCI push is tracked in
[#73](https://github.com/tuna-os/mandelbrot/issues/73) and not yet implemented; update this
document once that lands.

## Translations

Mandelbrot inherits Fractal's translations from the GNOME translation team on
[Damned Lies](https://l10n.gnome.org/module/fractal/) — see `po/*.po`. This fork does not run its
own translation-branch sync; `README.md`'s Contributing → Translations section has the current
pointer to where translators should work.
