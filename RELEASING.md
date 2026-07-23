# Releasing Fractal

## Before making a new release

* Update the dependencies (crates or system libraries) and migrate from deprecated APIs.
* Make the `build-stable` CI jobs use the latest stable GNOME runtime.

## Making a new stable release

1. If this is a new major version, create a new `fractal-M` branch, where `M` is the major version
   number.
2. Create a [release merge request](#release-merge-request-content) against the major version
   branch.
3. After the MR is merged, [create a tag](#creating-a-signed-tag) on the last commit of the major
   version branch.
4. Create a release on GitLab for that tag.
5. Make a fast-forward merge of the major version branch to `main`.
6. [Publish the new version on Flathub and Flathub beta](#publishing-a-version-on-flathub).
7. [Get the stable branch added to Damned Lies](#getting-a-branch-added-to-damned-lies).

## Making a new beta release

1. Create a [release merge request](#release-merge-request-content) against `main`.
2. After the MR is merged, [create a tag](#creating-a-signed-tag) on the last commit of `main`.
3. Create a release on GitLab for that tag.
4. [Publish the new version on Flathub beta](#publishing-a-version-on-flathub).

## Release merge request content

_To represent conditional list items, this section will start items with "**stable.**" to mean "if
this is a stable release"._

Make a single release commit containing the following changes:

* Update `/meson.build`:
  * Change the version on L3, it must look the same as it would in the app, with a
    `major_version.pre_release_version` format.
  * Change the `major_version` and `pre_release_version` on L13-14. For stable versions,
    `pre_release_version` should be an empty string.
* Update `/Cargo.toml`: change the `version`, using a semver format.
* Update `/README.md`:
  * **stable.** update the current stable version and its release date.
  * Update the current beta version. For stable versions, put `(same as stable)` instead of the
    release date.
* Update `/data/org.tunaos.mandelbrot.metainfo.xml.in.in`:
  * Add a new `release` entry at the top of the `releases`:
    * Its `version` should use the `major_version~pre_release_version` format.
    * For stable versions, its `type` should be `stable`, otherwise it should be `development`.
  * **stable.** remove all the `development` entries.
  * **stable.** update the paths of the screenshots to point to the major version branch.
* **stable.** If there were visible changes in the UI, update the screenshots in `/screenshots`.
  They can be generated with the [fractal-screenshots](https://gitlab.gnome.org/kcommaille/fractal-screenshots)
  repository and should follow [Flathub's quality guidelines](https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines/quality-guidelines#screenshots).

A good practice in this merge request is to launch the `build-stable` CI jobs to make sure that
Fractal builds with the stable Flatpak runtime.

## Creating a signed tag

Creating a signed tag is not mandatory but is good practice. To do so, use this command:

```sh
git tag -s V
```

With `V` being the version to tag, in the format `major_version.pre_release_version`.

You will be prompted for a tag message. This message doesn't really matter so something like
`Release Fractal V` should suffice.

## Publishing a version on Flathub

Publishing a version of Fractal on Flathub is done via its [Flathub repository on GitHub](https://github.com/flathub/org.tunaos.mandelbrot/).
A permission from the Flathub team granted to your GitHub account is necessary to merge PRs on this
repository, but anyone can open a PR.

* Open a PR against the correct branch. For a stable build, work against the `master` branch, for a
  beta build, work against the `beta` branch.

  It must contain a commit that updates the manifest to:

  * Use the latest GNOME runtime.
  * Make sure that the Flatpak dependencies are the same as in the nightly manifest, and using the
    same version.
  * Build the latest version of Fractal, identified by its tag _and_ commit hash.

  If the list of Rust modules to build changes, the `MODULES` variable in the
  `update-cargo-sources.sh` script must also be updated.
* When the PR is opened, a CI job will update the `*-cargo-sources.json` files with the latest
  dependencies for the Rust modules and add a commit to the PR if necessary.
* Trigger a test build by posting a comment saying `bot, build`.

  If the build succeeds, test the generated Flatpak as instructed and watch for obvious errors. If
  there are no issues, merge the PR.
* Merging the PR will trigger an "official" build that will then be published on Flathub or Flathub
  beta within 1 to 2 hours. If this build fails, an issue will be opened on the GitHub repository.
  The Flathub admins need to be contacted to launch it again.

More details about these steps can be found in the Flathub docs about [maintenance](https://docs.flathub.org/docs/for-app-authors/maintenance)
and [updates](https://docs.flathub.org/docs/for-app-authors/updates).

## Getting a branch added to Damned Lies

Damned Lies is the GNOME translation management platform. It provides translation workflows, but
also statistics. Even though we don’t publish any release from stable branches after the initial
one, we add them there so we can keep track of the evolution of translation coverage.

1. Go to <https://l10n.gnome.org/module/fractal/> and log in.
2. Click on the pencil icon next to the branch list.
3. In the entry at the bottom, type in the name of the new branch, then click on the Save button.
4. Assign the newly added branch to the “Other Apps (stable)” Release, unassign the previous one.
5. Hit Save again for the assignments to take effect.
