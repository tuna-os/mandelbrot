# Guide screenshots

Every PNG in this directory is generated, never hand-made. They are committed
so that a UI change shows up as an image diff in review, and so the landing
page and the metainfo can link at stable paths.

> **Status**: the pipeline below is in place but the first capture run has not
> happened yet, so this directory holds no PNGs and the image links in
> `docs/FEATURES.md`, `README.md` and the metainfo point at files that still
> need to be generated. Run the walkthrough (below) to fill them in.

## How they are made

`MANDELBROT_WALKTHROUGH=1` puts the app in walkthrough mode
([`src/walkthrough.rs`](../../src/walkthrough.rs)). It holds an ordered list of
named steps; a GLib timer runs each one, polls its readiness predicate until
the UI has settled, prints `WALKTHROUGH-SHOT <name>` to stdout, and quits after
the last one. [`scripts/walkthrough.sh`](../../scripts/walkthrough.sh) reads
those markers and grabs one `xwd` frame per marker into `docs/guide/<name>.png`.

Two passes:

| Pass | Trigger | Covers |
| --- | --- | --- |
| login | no credentials in the environment | greeter (incl. "Sign in with QR Code"), QR login, homeserver, login method, call prescreen + call view (via `CallState::start_demo`), the create-poll dialog, the error page, About |
| session | `MANDELBROT_WALKTHROUGH_USER`/`_PASSWORD`/`_HOMESERVER` set | room list, a populated timeline (messages, poll, voice message), the thread panel, the space overview, account settings, plus everything in the shared tail |

The session pass needs fixture content.
[`scripts/walkthrough-seed.sh`](../../scripts/walkthrough-seed.sh) creates it
against a local homeserver — three demo users with real display names, three
rooms with conversations, a disclosed poll with responses, a four-reply thread,
a voice message (`m.audio` + MSC3245/MSC1767) and a space with three children —
and prints the environment the driver needs. Rooms are deliberately
**unencrypted**: the screenshots do not need E2EE and skipping it takes
key-sharing and decryption timing out of the walkthrough.

## Reproducing locally

```sh
# 1. Bring up the local stack (synapse + livekit + lk-jwt).
(cd tests/e2e && podman-compose up -d)

# 2. Seed the fixtures.
scripts/walkthrough-seed.sh http://127.0.0.1:8008 > /tmp/walkthrough.env

# 3. Drive the app. Both passes, then validation.
WALKTHROUGH_ENV=/tmp/walkthrough.env scripts/walkthrough.sh ./target/debug/fractal
```

Add `WALKTHROUGH_SCHEME=dark WALKTHROUGH_SUFFIX=-dark` for the dark variants.

### On a headless build host

The app has to run inside its flatpak sandbox (it loads its GResources from
`/app`), and the sandbox has no X server. The arrangement that works:

* an `Xvfb :91` inside a container that shares the host's `/tmp`, so the socket
  in `/tmp/.X11-unix` is visible to both;
* the app started with
  `flatpak build --share=network --socket=x11 --filesystem=home --env=DISPLAY=:91 <builddir> /app/bin/fractal`
  — `--socket=x11` is what binds `/tmp/.X11-unix` into the sandbox;
* `GSK_RENDERER=cairo`, because llvmpipe under Xvfb otherwise produces blank
  frames.

The build itself uses the recipe from `MANDELBROT.md`:
`org.flatpak.Builder --stop-at=fractal <builddir> org.tunaos.mandelbrot.json`
once, then `flatpak build --share=network --filesystem=home <builddir> sh
<script>` for the meson/ninja/cargo steps.

## Validation

[`scripts/walkthrough-validate.py`](../../scripts/walkthrough-validate.py) runs
at the end of every capture. For each PNG it checks that the file exists, is
not a stub, has the expected dimensions, and is **not** blank — it counts
distinct colours and the fraction of pixels taken by the dominant colour, which
is what catches the failure mode where the app starts, prints all its markers,
and renders a solid grey rectangle. It exits non-zero listing every bad shot.

CI (`.github/workflows/ci.yml`, job "Screenshot walkthrough") runs the login
pass on every PR and fails if the final `about` marker never appears or if any
screenshot fails validation. The session pass is **not** a PR gate: it needs
the `tests/e2e` container stack, which the flatpak CI container cannot nest.
Regenerate the session shots by hand when the feature UI changes.
