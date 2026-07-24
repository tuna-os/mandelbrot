<div align="center">

<img src="data/icons/org.tunaos.mandelbrot.svg" alt="" />

# Mandelbrot

</div>

Mandelbrot is a Matrix messaging app for GNOME written in Rust, forked from
[Fractal](https://gitlab.gnome.org/World/fractal/) to push further into the modern Matrix feature
set: native MatrixRTC voice/video calling, simplified sliding sync, QR login, threads, and more.
Its interface is optimized for collaboration in large groups, such as free software projects, and
will fit all screens, big or small.

Highlights:

* Find rooms to discuss your favorite topics, or talk privately to people, securely thanks to
  end-to-end encryption
* Send rich formatted messages, files, voice messages, polls, or your current location
* Reply to specific messages — in the room or in threads — react with emoji, edit or remove messages
* View images, and play audio and video directly in the conversation
* See who has read messages, and who is typing
* Log into multiple accounts at once, with Single-Sign On, OAuth 2.0, and QR code login

New in Mandelbrot (see [docs/FEATURES.md](docs/FEATURES.md) for details):

* **Native voice & video calls** (experimental) — MatrixRTC/LiveKit built in, end-to-end
  encrypted, with GNOME-native call UI and call notifications; no embedded browser
* **Simplified sliding sync** (MSC4186) with automatic classic-sync fallback
* **QR code login** (MSC4108) — sign in by scanning, or link a new device
* **Threads** (MSC3440) with an adaptive thread panel
* **Polls** (MSC3381) — create, vote, end
* **Voice messages** (MSC3245) — record and send

## Installation

Install from the TunaOS flatpak remote:

```sh
flatpak remote-add --if-not-exists tuna-os https://tunaos.org/flatpak/tuna-os.flatpakrepo
flatpak install tuna-os org.tunaos.mandelbrot
```

## Relationship to Fractal

Mandelbrot is a friendly downstream fork of GNOME Fractal (GPL-3.0-or-later). All Fractal
functionality is retained, and we aim to track upstream releases. See [MANDELBROT.md](MANDELBROT.md)
for the roadmap and the feature gap analysis. Please report issues with core messaging that also
reproduce in Fractal to the [Fractal project](https://gitlab.gnome.org/World/fractal/-/issues), and
Mandelbrot-specific issues (calls, sliding sync, QR login…) to this repo.

## Security Best Practices

You should use a strong **password** that is hard to guess to protect the secrets stored on your
device, whether the password is used directly to unlock your secrets (with a password manager for
example) or if it is used to open your user session and your secrets are unlocked automatically
(which is normally the case with a GNOME session).

Furthermore, make sure to lock your system when stepping away from the computer since an unlocked
computer can allow other people to access your private communications and your secrets.

## Contributing

### Code

Please follow our [contributing guidelines](CONTRIBUTING.md).

### Translations

Fractal is translated by the GNOME translation team on [Damned lies](https://l10n.gnome.org/).

Find your language in the list on [the Fractal module page on Damned lies](https://l10n.gnome.org/module/fractal/).

The names of the emoji displayed during verification come from [the Matrix specification repository](https://github.com/matrix-org/matrix-spec/tree/main/data-definitions).
They are translated on [Element’s translation platform](https://translate.element.io/projects/matrix-doc/sas-emoji-v1).

## Frequently Asked Questions

Does Fractal have encryption support?

: **Yes**, since Fractal 5, encryption is supported using Cross-Signing. See
  <https://gitlab.gnome.org/World/fractal/-/issues/717> for more info on the state of encryption.

Can I run Fractal with the window closed?

: Currently Fractal does not support this. Fractal is a GNOME application, and accordingly adheres to
  the GNOME guidelines and paradigms. This will be revisited [if or when GNOME gets a proper paradigm
  to interact with apps running in the background](https://gitlab.gnome.org/World/fractal/-/issues/228#note_2054826).

## The origin of Fractal

The current version is a complete rewrite of Fractal built on top of the
[matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) using [GTK4](https://gtk.org/).

The previous version of Fractal was using GTK3 and its own backend to talk to a matrix homeserver,
the code can be found in the [`legacy` branch](https://gitlab.gnome.org/World/fractal/-/tree/legacy).

Initial versions were based on Fest <https://github.com/fest-im/fest>, formerly called ruma-gtk.
In the origins of the project it was called guillotine, based on French revolution, in relation with
the Riot client name, but it's a negative name so we decide to change for a math one.

The name Fractal was proposed by Regina Bíró.

## Code of Conduct

Fractal follows the official [GNOME Code of Conduct](https://conduct.gnome.org/).
