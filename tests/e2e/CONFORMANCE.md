# MatrixRTC conformance report

Results of running Mandelbrot's native MatrixRTC implementation against a live
stack and against Element Call. Re-run with `run-interop.sh` (client ↔ client)
and `run-interop-ec.sh <scenario> <mode>` (client ↔ Element Call).

Last run: 2026-07-24, against Element Call **v0.22.0** (`ae7ede32`, built
2026-07-20, `ghcr.io/element-hq/element-call:latest`) on synapse + livekit +
lk-jwt-service.

## Summary

Mandelbrot interoperates with deployed Element Call. Membership events are
shape-identical, both clients read each other's memberships, calls connect in
both directions, and crash cleanup works across implementations. The one
unsupported surface is the future MSC4354 sticky-event mode, which is a known
deferred item, not a regression.

## The state-key format question — resolved

Element Call v0.22.0 writes the **same device-keyed slot format we emit**:

```
type      org.matrix.msc3401.call.member
state_key _{user_id}_{device_id}_m.call
```

The historical user-keyed format (`@user:server` with a `memberships[]` array)
could not be produced at all: even with `matrix_rtc_mode: "legacy"` *and*
`feature_use_device_session_member_events: false` (config verified as served),
EC still wrote the device-keyed key. It is not reachable by configuration in
this release.

**Conclusion:** keep emitting `m.call.member` with `SessionMembershipData` and
the device-keyed state key. Reading that format alone is sufficient for
deployed Element Call today.

## Scenario matrix

| Element Call config | EC writes | we read EC | EC reads us | result |
|---|---|---|---|---|
| default, EC joins first | device-keyed, focus `multi_sfu` | yes | yes | 8/8 pass |
| default, we join first | same | yes | yes | 7/7 pass |
| default, encrypted room, we join first | same | yes | yes | 7/7 pass |
| `legacy` + device-events off | device-keyed, focus `oldest_membership` | yes | yes | 8/8 pass |
| `matrix_2_0` | sticky timeline `org.matrix.msc4143.rtc.member` | **no** | yes | 2 fail / 6 pass |

Delayed-leave (MSC4140) interop passed in every mode: `kill -9` on our client
removed our tile from Element Call within 7–10 s; rejoining restored it; a
graceful SIGINT removed it immediately.

## Wire-level differences

Identical between implementations: `application`, `call_id`, `device_id`,
`foci_preferred[]` (type, alias, service URL), `membershipID`, `scope`, and the
absence of `created_ts`. `expires` differs only by configuration (ours 4 h,
EC's 50 h).

**One content-level divergence:** `focus_active.focus_selection`. We always
send `oldest_membership`; EC's default mode sends `multi_sfu` (its legacy mode
sends `oldest_membership`, matching us). This was harmless in every run —
consider aligning per mode.

## Media E2EE: not proven by this harness (harness limitation)

Our client logged `MissingKey` for every Element Call track, in every config
including an encrypted room. The cause was established directly rather than
inferred: Element Call made **zero `/sendToDevice/` calls**, because
`/keys/query` showed our harness client had published **no device keys** —
the `join_call` example is a bare HTTP client with no Olm stack, so EC found no
Olm-capable device and skipped key distribution entirely. The failure is
upstream of any decryption logic.

Element Call's actual requirement is only that a peer device has published
device keys; no verification or cross-signing step was reached. The Mandelbrot
app satisfies this — matrix-sdk publishes device keys and
`src/session/call/client_api.rs` sends keys with
`encrypt_and_send_raw_to_device(..., CollectStrategy::AllDevices)`. So
app-to-EC media E2EE is expected to work but **remains unproven**; proving it
requires driving the real app rather than the example client.

Element Call rendered our participant correctly (padlock, participant count 2,
audio-only avatar tile — correct, as we publish a silent microphone and no
camera).

## MSC4354 sticky events (`matrix_2_0`) — the real compatibility gap

In this mode EC sends a sticky **timeline** event instead of room state:

```
PUT /rooms/{room}/send/org.matrix.msc4143.rtc.member/{txn}
    ?org.matrix.msc4140.delay=8000&org.matrix.msc4354.sticky_duration_ms=3600000
{"application":{"type":"m.call"}, "slot_id":"m.call#ROOM",
 "rtc_transports":[{"type":"livekit","livekit_service_url":"..."}],
 "member":{"device_id":"...","user_id":"...","id":"<uuid>"},
 "versions":[], "msc4354_sticky_key":"<uuid>"}
```

It fetches the SFU JWT from `/get_token` only (never `/sfu/get`) and uses
**hashed LiveKit identities**. EC still renders us (it reads legacy state for
back-compat) and we still reach the same SFU room and subscribe to its tracks,
but the hashed identity means our `{user}:{device}` key mapping would not match
even with keys present.

This — not the legacy user-keyed format — is the compatibility work that
matters next. The membership manager is already behind a trait so the sticky
variant can be added alongside.

## Element Call CI parity

Element Call gates its releases on `pnpm test` (89 vitest files) and
`pnpm exec playwright test` (22 specs, chromium + firefox + a mobile
project, against the `docker-compose-dev.yml` + `docker-compose-playwright.yml`
two-homeserver stack). The table below maps every Playwright spec to our
coverage.

**UI-only** means the spec asserts on rendered tiles, buttons, focus order or
the Element Web widget host. We have no UI in this harness, so there is
nothing to port — the underlying engine behaviour, where there is any, is
listed separately.

| Element Call spec | asserts | our coverage |
|---|---|---|
| `landing.spec.ts` | page title, login button, home form | n/a (UI-only) |
| `access.spec.ts` | register / login / logout; guest invite link, 2 tiles | n/a (UI + EC's own guest flow). The 2-participant equivalent is `run-interop.sh`. |
| `create-call.spec.ts` | lobby → in-call, participant count, feedback screen; unmute-in-lobby bugfix | n/a (UI-only) |
| `errors.spec.ts` | OpenID 418 → error screen; 429 → retried; bad SFU URL → "Failed to create call" | **added** — `run-restricted-sfu.sh` scenarios 2 and 3 (auth service unreachable, then recovery). The error *screens* are UI-only. |
| `restricted-sfu.spec.ts` | `/sfu/get` completes **before** the `m.call.member` state event; focus pre-warm errors hit the ErrorBoundary | **added** — `run-restricted-sfu.sh` scenario 1. **We do not hold this property** — see "Findings" below. |
| `reconnect.spec.ts` | homeserver stalled → "Reconnecting…" dialog; tab order restricted to header/footer | **added** — `run-resilience.sh` scenario 2 (stall → `ProbablyLeft` → recovery) and `probably_left_*` in `matrixrtc/tests/element_call_scenarios.rs`. Tab order is UI-only. |
| `sfu-reconnect-bug.spec.ts` | creator leaves → no new LiveKit websocket on the remaining guest (EC #3344) | **added** — `run-resilience.sh` scenario 1 + `sfu_reconnect_*` unit tests. Passes. |
| `spa-call-sticky.spec.ts` | MSC4354 sticky membership events; rejoin with a new `member.id` does not crash | **not applicable / known unsupported** — pinned by `sticky_*` unit tests so support landing is noticed. |
| `mobile/create-call-mobile.spec.ts` | Pixel-7 viewport call flow | n/a (UI-only) |
| `widget/simple-create.spec.ts` | start/join/leave a call as an Element Web widget | n/a (widget host). Join/leave is `run-interop.sh`. |
| `widget/huddle-call.test.ts` | 5 participants, 5 tiles everywhere, mute reflected | **added** — `run-huddle.sh` (memberships, key fan-out, track fan-out, leave) + `huddle_*` unit tests. |
| `widget/screen-share.test.ts` | second video track per participant, spotlight switch, indicators | **cannot** — `join_call` publishes one silent audio track and no screen share. Nothing in the membership/key layer changes for a second track (LiveKit track source only), so the engine risk is low. |
| `widget/voice-call-dm.spec.ts` | DM ringing, voice vs. video defaults, hang-up ends both sides | **partial** — the ring notification is covered by `call_session.rs` (`m.rtc.notification`); intents by `dm_*` unit tests. The toast/composer assertions are UI-only. |
| `widget/pip-call.test.ts`, `widget/pip-call-button-interaction.test.ts` | picture-in-picture layout and `object-fit` | n/a (UI-only) |
| `widget/federated-call.test.ts` | federated call in 4 rtc-mode pairs, 2 tiles each side | **added** — `run-federation.sh` (both join orders) + `federated_call_*` unit tests. The `compat`/`multi_sfu` pairs are not applicable: we only implement `oldest_membership`. |
| `widget/federation-oldest-membership-bug.spec.ts` | new joiner publishes on the **oldest membership's** SFU across homeservers, even when that JWT service is slow | **added** — `run-federation.sh` + `federation_oldest_membership_*` unit tests. Passes both directions. |
| `widget/hotswap-legacy-compat.test.ts` | publisher recreation when the local focus switches (deadlock regression) | **not applicable** — we never switch focus mid-call (see "Findings"). Pinned by `hotswap_*`. |

Element Call's own **unit** suite covers a large surface with no counterpart
in ours, because we ported `matrix-js-sdk`'s `matrixrtc` suite rather than
Element Call's application code. Uncovered EC concepts, in rough order of
engine relevance:

- `src/state/CallViewModel/localMember/RtcTransportAutoDiscovery.test.ts`,
  `LocalTransport.test.ts` — transport discovery and switching. We resolve
  one focus at connect and never revisit it.
- `src/state/CallViewModel/localMember/Publisher.test.ts`,
  `remoteMembers/ConnectionManager.test.ts`, `Connection.test.ts`,
  `ECConnectionFactory.test.ts`, `integration.test.ts` — the multi-SFU
  connection manager. Not applicable while we are single-SFU.
- `src/e2ee/matrixKeyProvider.test.ts` — key provider wiring. Our equivalent
  is `encryption_manager.rs` (19 tests) plus the harness key assertions.
- `src/state/CallViewModel/localMember/HomeserverConnected.test.ts` — the
  "Reconnecting…" state machine. Ours is `probably_left_*`.
- Everything else (`MediaViewModel`, `LayoutSwitch`, `MuteStates`, reactions,
  rageshake, analytics, ~60 component tests) is application UI.

## Harness runs (2026-07-24)

Executed against the live stack (podman, join_call built with the `livekit`
feature). `run-restricted-sfu.sh` and `run-huddle.sh` are written and
syntax-checked but **were not executed**: the build host became unreachable
mid-session. They share `harness-lib.sh` and the same structure as the two
suites that did run.

Two caveats for whoever runs them first:

- `run-huddle.sh 5` starts five `join_call` processes, each a ~250 MB binary
  carrying libwebrtc. That is heavy for a standard CI runner — it is the
  class of load that took the build host down during this session. Start
  with `./run-huddle.sh 3` on a small runner.
- Everything here was exercised under **rootless podman**. The nginx configs
  use `host.containers.internal` (podman's host-gateway name; Docker calls it
  `host.docker.internal`), and `harness-lib.sh`'s `detect_compose` probes
  `podman compose` before `docker compose`. The docker-on-runner path in
  `.github/workflows/matrixrtc-e2e.yml` is **unverified**; the first
  scheduled run is a shakeout.

`run-federation.sh remote-first` and `run-federation.sh local-first` —
**10/10 PASS each**:

```
PASS federation-room-join           timo (site 2) joined a room created on site 1
PASS second-joiner-connects
PASS same-sfu                       both on ws://127.0.0.1:17880 (resp. :7880)
PASS follows-oldest-membership-sfu  second joiner uses the creator's SFU
PASS cross-site-msc4195             token from the *other* site's auth service
PASS memberships-replicate          both m.call.member events on both sites
PASS <first>-subscribes / -receives-keys
PASS <second>-subscribes / -receives-keys
```

`run-resilience.sh` — **11 PASS, 1 known gap, 1 follow-on**:

```
PASS two-memberships / shared-focus
PASS no-sfu-reconnect-on-creator-leave   survivor stayed on its connection (1)
PASS no-jwt-refetch-on-creator-leave     jwt requests stayed at 0
PASS survivor-still-joined / survivor-alive
PASS probably-left-on-stall              ProbablyLeft(true) after a 14 s stall
PASS recovers-membership-after-stall
PASS survives-homeserver-stall
PASS probably-left-does-not-flap         1 emission for one stall
PASS notices-sfu-restart
INFO no-sfu-reconnect-known-gap          the client ended the call (finding 2)
PASS clean-teardown-after-sfu-loss       no stale membership
PASS final-graceful-leave
```

The last three lines are the re-labelled form: the first run recorded
`FAIL membership-survives-sfu-restart` and `FAIL final-graceful-leave`, which
is how finding 2 was discovered.

## Findings from the new suites

**1. We publish our membership before we can prove SFU access.** Element
Call's `restricted-sfu.spec.ts` exists to guarantee the opposite order:
`/sfu/get` must succeed before `m.call.member` is sent, so that on a
deployment where call creation is restricted, an unauthorised client never
advertises a participant it cannot back with media. Mandelbrot does it the
other way round in both the example client and the app:
`RtcCallSession::join_rtc_session` publishes the state event, and only then
does the media path resolve the focus (`src/session/call/media.rs`
`service_url` → `fetch_sfu_config` → `LivekitCallConnection::connect`). The
resolution order is in fact forced by our design: `get_active_focus` reads
the *oldest membership*, which requires the room state to already contain
memberships. Element Call avoids the circularity by pre-warming its own
preferred focus first. Not fixed here — it is app/engine architecture, not a
test bug. `run-restricted-sfu.sh` records it as a FAIL line so it cannot be
forgotten.

The exposure is bounded, though: the membership is scheduled with an MSC4140
delayed leave, so a client that fails to obtain a token disappears again
within the delay window (~8 s in this harness). On a restricted deployment
this is a transient wrong state, not a permanently stranded participant.

**2. No SFU reconnect.** On a LiveKit `RoomEvent::Disconnected`, both
`matrixrtc/examples/join_call.rs` and `src/session/call/media.rs` break out of
their media loop and tear the connection down; neither retries. Restarting
the SFU under a live call therefore ends the call. Element Call has no e2e for
this because `livekit-client` reconnects automatically. `run-resilience.sh`
scenario 3 records it as a known gap and asserts only that we tear down
cleanly (no stale membership) — which we do.

**3. `ProbablyLeft` flapping is upstream behaviour, not our bug.** The open
item below is resolved. `MembershipManager::set_and_emit_probably_left`
deduplicates, so no value is ever emitted twice in a row; a real
`true → false → true` oscillation happens only when the homeserver answers
delayed-event restarts *slower than* `delayed_leave_event_restart_local_timeout_ms`
(2 s) but the retry then succeeds. Each restart interval produces one
transition pair. This is a verbatim port of `matrix-js-sdk`'s
`MembershipManager.restartDelayedEvent`, so any hysteresis belongs upstream
or in the UI. Reproduced deterministically by
`probably_left_flaps_against_a_chronically_slow_homeserver`; a clean 14 s
homeserver stall against the live stack produced exactly **one** emission
(`run-resilience.sh`), i.e. no flapping under normal conditions.

**4. Federation works end to end.** Two homeservers, two SFUs, two auth
services: the second joiner correctly follows the oldest membership onto the
*other* site's SFU, obtains an MSC4195 token from that site's
`lk-jwt-service` using an OpenID token issued by its own homeserver, and
media plus encryption keys flow both ways. 10/10 in both join orders. This is
the scenario Element Call had a bug in.

## Open items

- Prove app-level media E2EE against Element Call by driving the real app.
- Consider matching `focus_selection` to the peer mode.
- Decide whether to fetch the SFU token before advertising membership
  (finding 1) and whether to grow an SFU reconnect path (finding 2).
- MSC4354 sticky events remain unsupported (see above).
