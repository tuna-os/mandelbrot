# MatrixRTC interop e2e tests

End-to-end conformance harness for the native MatrixRTC implementation in
[`matrixrtc/`](../../matrixrtc) (see the "MatrixRTC conformance testing"
section of `MANDELBROT.md`). It spins up a real homeserver + SFU stack and
proves, over the wire, that our client produces spec-conformant MatrixRTC
sessions (MSC4143 memberships, MSC4140 delayed leave, MSC4195 SFU auth,
`io.element.call.encryption_keys` key transport, E2EE media).

## Contents

| Path                    | Purpose                                                              |
| ----------------------- | -------------------------------------------------------------------- |
| `compose.yml`           | Minimal single-homeserver stack (synapse, nginx, livekit, lk-jwt)    |
| `synapse/homeserver.yaml` | Synapse config: MSC4140 (`max_event_delay_duration`), MSC4222, MSC4354, open registration |
| `livekit/livekit.yaml`  | LiveKit SFU config (host networking, dev key `devkey:secret`)        |
| `nginx/nginx.conf`      | TLS at `synapse.m.localhost` (needed by lk-jwt federation OpenID check) + plain-http client access on host port 8008 |
| `tls/`                  | Self-signed `*.m.localhost` dev certificates, copied verbatim from element-call's `backend/` (TESTING ONLY) |
| `run-interop.sh`        | TEST 1: two native clients in the same call, full assertion suite    |

The configs are adapted from element-call's `docker-compose-dev.yml` +
`backend/` dev fixtures, minimized to a single site.

## Architecture notes

- **Clients talk plain HTTP/WS**: our test client uses webpki roots
  (rustls), which cannot trust the self-signed dev CA, so synapse is
  reached at `http://127.0.0.1:8008` (nginx port 80), the JWT service at
  `http://127.0.0.1:6080` and the SFU at `ws://127.0.0.1:7880`. TLS exists
  *only* inside the compose network because `lk-jwt-service` validates
  MSC4195 OpenID tokens via the Matrix federation API, which
  gomatrixserverlib only speaks over https (hence nginx +
  `LIVEKIT_INSECURE_SKIP_VERIFY_TLS`).
- **LiveKit runs with host networking** so its ICE candidates are
  reachable by clients on the same host. With rootless podman, container
  IPs are not routable from the host and the SFU would advertise
  unreachable candidates.
- Rootless podman works; no ports below 1024 are published.

## Running

Prerequisites: `podman-compose` (or `docker compose`), `curl`, `jq`, and a
Rust toolchain able to build the `matrixrtc` crate with the `livekit`
feature (needs a C/C++ toolchain for libwebrtc; on a pkg-config-poor host
use a distrobox).

```sh
# Optionally prebuild the client:
(cd ../../matrixrtc && cargo build --features livekit --example join_call)

JOIN_CALL_BIN=../../matrixrtc/target/debug/examples/join_call ./run-interop.sh
```

`KEEP_STACK=1` keeps the containers running afterwards for debugging.
Client and state logs land in `logs/<timestamp>/`.

### What TEST 1 asserts

1. Both clients publish an `org.matrix.msc3401.call.member` state event
   whose content is well-formed `SessionMembershipData`:
   `application: "m.call"`, `call_id: ""`, a `device_id`,
   `focus_active: {type: livekit, focus_selection: oldest_membership}` and
   at least one `foci_preferred` entry with a `livekit_service_url`.
2. Each client sees the other's membership (via room state).
3. Each client receives the other's media encryption key (to-device
   `io.element.call.encryption_keys`; **unencrypted** in this harness —
   the example does not carry an Olm stack).
4. Each client subscribes to the other's audio track and receives
   *decrypted* audio frames (the LiveKit frame cryptor drops frames it
   cannot decrypt, so frame delivery proves the E2EE key exchange).
5. `kill -9` on one client: the homeserver fires the MSC4140 delayed
   leave event and the membership content becomes `{}` within the delay
   window (8 s delay + 5 s keep-alive interval + slack).
6. SIGINT on the other client: graceful leave clears the membership
   immediately.

## CI design (future)

This is intended to become a **manual/nightly** CI job, not per-MR:

- it pulls ~1.5 GB of container images and builds libwebrtc (~10 min cold),
- it needs a runner with rootless podman and UDP ports 50100-50200,
- media timing makes it inherently flakier than the unit suite
  (`matrixrtc/tests/`), which remains the merge gate.

Sketch: a scheduled GitLab/GitHub job that caches the cargo target dir,
runs `./run-interop.sh` and uploads `logs/` as an artifact on failure.

## Phase 2 (planned): Element Call interop

Join the *same* call with a headless Element Call to prove interop with
the reference client, modeled on element-call's Playwright setup:

- add the `ghcr.io/element-hq/element-call` container (config pointing at
  `http://127.0.0.1:8008` + `livekit_service_url` `http://127.0.0.1:6080`;
  Chromium treats `localhost` as a secure context, so plain http/ws works),
- drive it with Playwright Chromium (`--use-fake-device-for-media-stream`),
  log in as bob, open the room call,
- assert mutual membership (state API), media both ways, and EC-side
  reception of *our* keys. NOTE: Element Call sends its keys via
  **Olm-encrypted** to-device messages when the room is encrypted; the
  plain harness client can only interop in unencrypted rooms, and phase 2
  must verify which key-transport mode EC uses there (it may still refuse
  unencrypted transport — in that case the harness needs an Olm stack via
  matrix-sdk-crypto).
