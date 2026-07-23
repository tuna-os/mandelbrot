# Contributing

On this page you can learn how to contribute to [Fractal](https://gitlab.gnome.org/World/fractal/)
by working on the code.

## Getting Started

Here are a few links to help you get started with Rust and the GTK Rust bindings:

* [Learn Rust](https://www.rust-lang.org/learn)
* [GUI development with Rust and GTK 4](https://gtk-rs.org/gtk4-rs/stable/latest/book)
* [gtk-rs website](https://gtk-rs.org/)

[The Rust docs of our application](https://gnome.pages.gitlab.gnome.org/fractal/) and the [GNOME Development Center](https://developer.gnome.org/)
might also be useful.

Don't hesitate to join [our Matrix room](https://matrix.to/#/#fractal:gnome.org) to come talk to us
and ask us any questions you might have. The [“Rust ❤️ GNOME” room](https://matrix.to/#/#rust:gnome.org)
can also provide general help about using Rust in GNOME.

## Build Instructions

### Prerequisites

Fractal is written in Rust, so you will need to have at least Rust (the minimum required version is
available in the `Cargo.toml` file as `package.rust-version`) and Cargo available on your system.
You will also need to install the Rust nightly toolchain to be able to run our
[pre-commit hook](#pre-commit), which can be done with:

```sh
rustup toolchain install nightly
```

If you are building Fractal with Flatpak (via GNOME Builder or the command line), you will need to
manually add the necessary remotes and install the Rust freedesktop.org extension:

```sh
# Add Flathub and the gnome-nightly repo
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists gnome-nightly https://nightly.gnome.org/gnome-nightly.flatpakrepo

# Install the gnome-nightly Sdk and Platform runtime
flatpak install --user gnome-nightly org.gnome.Sdk//master org.gnome.Platform//master

# Install the required rust-stable extension from Flathub
flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//25.08
```

If you are building the flatpak manually you will also need flatpak-builder on your system, or the
`org.flatpak.Builder` flatpak from Flathub.

### GNOME Builder

Using [GNOME Builder](https://apps.gnome.org/Builder/) with [Flatpak](https://flatpak.org/) is
the recommended way of building and installing Fractal.

You can find help on cloning and building a project in the [docs of Builder](https://builder.readthedocs.io/).

To open a build terminal to run commands like [Clippy](#pre-commit), you can use the “+” button at
the left of the header bar of the editor and select “New build terminal”, or use its keyboard
shortcut <kbd>Shift</kbd>+<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>T</kbd>. The terminal should open in
the `_build` directory.

### Foundry

As an alternative, [Foundry](https://gitlab.gnome.org/GNOME/foundry) is a command line tool with
a lot of features similar to an IDE, which we can also use to develop in a Flatpak environment. It
should be available as the `foundry` package in your distribution.

First, set up the project:

```sh
foundry init
```

Then, you can build and run the application directly:

```sh
foundry run
```

_Note that Foundry will use `.foundry/cache/build` as build directory._

To test changes you make to the code, re-run that last command.

To run commands like [Clippy](#pre-commit) in the build environment, use:

```sh
foundry devenv -- {COMMAND}
```

The command will run in the `.foundry/cache/build` directory by default.

### fenv

Another command line alternative is [fenv](https://gitlab.gnome.org/ZanderBrown/fenv), which focuses
only on developing with Flatpak.

First, install fenv:

```sh
cargo install --git https://gitlab.gnome.org/ZanderBrown/fenv fenv
```

After that, set up the project:

```sh
# Set up the flatpak environment
fenv gen build-aux/org.tunaos.mandelbrot.Devel.json
```

Finally, build and run the application:

```sh
# Build the project
fenv build

# Launch Fractal
fenv run
```

_Note that fenv will use `_build` as build directory._

To test changes you make to the code, re-run these two last commands.

To run commands like [Clippy](#pre-commit) in the build environment, use:

```sh
fenv exec -- {COMMAND}
```

The command will run in the current directory by default.

### Install the flatpak

Some features that interact with the system require the app to be installed to test them (i.e.
notifications, command line arguments, etc.).

GNOME Builder can export a flatpak of the app after it has been successfully built.

Fractal can then be installed with:

```sh
flatpak install --user --bundle path/to/org.tunaos.mandelbrot.Devel.flatpak
```

Alternatively, it can be built and installed with flatpak-builder:

```sh
flatpak-builder --user --install app build-aux/org.tunaos.mandelbrot.Devel.json
```

_Note that the `flatpak-builder` command can be replaced with `flatpak run org.flatpak.Builder`._

It can then be entirely removed from your system with:

```sh
flatpak remove --delete-data org.tunaos.mandelbrot.Devel
```

### GNU/Linux

If you decide to ignore our recommendation and build on your host system, outside of Flatpak, you
will need Meson and Ninja.

```sh
meson setup --prefix=/usr/local _build
ninja -C _build
sudo ninja -C _build install
```

## Pre-commit

We expect all code contributions to be correctly formatted. To help with that, a pre-commit hook
should get installed as part of the building process. It runs the `hooks/checks` crate. It's a
quick script that makes sure that the code is correctly formatted with `rustfmt`, among other
things. Make sure that this script is effectively run before submitting your merge request,
otherwise CI will probably fail right away.

You should also run [Clippy](https://doc.rust-lang.org/stable/clippy/index.html) as that will catch
common errors and improve the quality of your submissions and is once again checked by our CI. To
reuse the same cache as when building Fractal, you should run the following command in a build
environment:

```sh
meson compile -C {BUILD_DIRECTORY} src/cargo-clippy
```

_ The `-C {BUILD_DIRECTORY}` option can be omitted when the command is run from the build
directory._

## Commit

Please follow the [GNOME commit message guidelines](https://handbook.gnome.org/development/commit-messages.html).
We enforce the use of a tag as a prefix for the summary line. It should be the area of the app that
is changed.

## Merge Request

You must pass all the prerequisites of the [Change Submission Guide](https://handbook.gnome.org/development/change-submission.html).

Before submitting a merge request, make sure that [your fork is available publicly](https://gitlab.gnome.org/help/user/public_access.md),
otherwise CI won't be able to run.

Use the title of your commit as the title of your MR if there's only one. Otherwise it should
summarize all your commits. If your commits do several tasks that can be separated, open several
merge requests.

In the details, write a more detailed description of what it does. If your changes include a change
in the UI or the UX, provide screenshots in both light and dark mode, and/or a screencast of the
new behavior.

Don't forget to mention the issue that this merge request solves or is related to, if applicable.
GitLab recognizes the syntax `Closes #XXXX` or `Fixes #XXXX` that will close the corresponding
issue accordingly when your change is merged.

We expect to always work with a clean commit history. When you apply fixes or suggestions,
[amend](https://git-scm.com/docs/git-commit#Documentation/git-commit.txt---amend) or
[fixup](https://git-scm.com/docs/git-commit#Documentation/git-commit.txt---fixupamendrewordltcommitgt)
and [squash](https://git-scm.com/docs/git-rebase#Documentation/git-rebase.txt---autosquash) your
previous commits that you can then [force push](https://git-scm.com/docs/git-push#Documentation/git-push.txt--f).

## LLM Contributions

Contributions must not include content generated by large language models or other probabilistic
tools like ChatGPT, Claude, and Copilot.

This policy exists due to

* ethical concerns about the data gathering for training these models
* the disproportionate use of electricity and water of building / running them
* the potential negative influence of LLM-generated content on quality
* potential copyright violations

This ban of LLM-generated content applies to all parts of the projects, including, but not limited
to, code, documentation, issues, and artworks. Translating texts for issues and comments to
English can be achieved with machine translation tools, without the use of generative AI (LLM),
such as DeepL.

### Project-related use of LLMs

We heavily discourage the use of LLM chat bots as a replacement for reading Fractal's documentation
and API reference.

Support requests referencing misleading or false LLM output relating to the project may be ignored,
since it is a waste of time for us to "debug" where things went wrong based on this output before
human support was sought.
