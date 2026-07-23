// SPDX-License-Identifier: GPL-3.0-or-later

//! Example: join the MatrixRTC room call of a room and connect to the
//! LiveKit SFU.
//!
//! ```sh
//! cargo run --features livekit --example join_call -- \
//!     https://matrix.example.org @alice:example.org DEVICEID ACCESS_TOKEN '!room:example.org'
//! ```
//!
//! This is a development harness, not a complete client:
//! - To-device messages are sent WITHOUT encryption (the real client must
//!   use Olm via its crypto stack), so media of other participants will not
//!   be decryptable by clients that require encrypted key transport.
//! - It publishes a silent microphone track instead of capturing a device.
//! - It syncs the call member state only once at startup.

#![allow(clippy::too_many_lines, clippy::large_futures)]

use std::process::exit;
use std::sync::Arc;
use std::time::Duration;

use mandelbrot_matrixrtc::livekit_connection::{
    LivekitCallConnection, OpenIdToken, fetch_sfu_config,
};
use mandelbrot_matrixrtc::{
    CallMembershipIdentity, ClientError, MemberStateEvent, RtcCallSession, RtcCallSessionConfig,
    RtcCallSessionEvent, RtcClientApi, RtcRoom, SendDelayedEventResponse, SendEventResponse,
    ToDeviceTarget, UpdateDelayedEventAction,
};
use ruma::{OwnedEventId, RoomId, events::StateEventType};
use serde_json::{Value as JsonValue, json};

/// A minimal, plain-HTTP implementation of [`RtcClientApi`] against the
/// Matrix client-server API. Development only.
struct HttpClientApi {
    http: reqwest::Client,
    homeserver: String,
    access_token: String,
    user_id: String,
}

impl HttpClientApi {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.homeserver.trim_end_matches('/'))
    }

    fn txn_id() -> String {
        format!(
            "mandelbrot{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    }

    async fn request(
        &self,
        method: reqwest::Method,
        url: String,
        query: &[(String, String)],
        body: &JsonValue,
    ) -> Result<JsonValue, ClientError> {
        let response = self
            .http
            .request(method, url)
            .bearer_auth(&self.access_token)
            .query(query)
            .json(body)
            .send()
            .await
            .map_err(|e| ClientError::Other(e.to_string()))?;

        let status = response.status().as_u16();
        let value: JsonValue = response.json().await.unwrap_or_default();
        if (200..300).contains(&status) {
            return Ok(value);
        }

        let errcode = value
            .get("errcode")
            .and_then(JsonValue::as_str)
            .unwrap_or("M_UNKNOWN");
        if status == 404 && errcode == "M_UNRECOGNIZED" {
            return Err(ClientError::UnsupportedDelayedEventsEndpoint);
        }
        Err(ClientError::Matrix {
            errcode: errcode.to_owned(),
            http_status: Some(status),
            retry_after: value
                .get("retry_after_ms")
                .and_then(JsonValue::as_u64)
                .map(Duration::from_millis),
            max_delay: value
                .get("org.matrix.msc4140.max_delay")
                .and_then(JsonValue::as_u64)
                .map(Duration::from_millis),
        })
    }

    async fn get_openid_token(&self) -> Result<OpenIdToken, ClientError> {
        let value = self
            .request(
                reqwest::Method::POST,
                self.url(&format!(
                    "/_matrix/client/v3/user/{}/openid/request_token",
                    self.user_id
                )),
                &[],
                &json!({}),
            )
            .await?;
        serde_json::from_value(value).map_err(|e| ClientError::Other(e.to_string()))
    }

    async fn get_call_member_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<MemberStateEvent>, ClientError> {
        let value = self
            .request(
                reqwest::Method::GET,
                self.url(&format!("/_matrix/client/v3/rooms/{room_id}/state")),
                &[],
                &json!({}),
            )
            .await?;
        let events = value.as_array().cloned().unwrap_or_default();
        Ok(events
            .iter()
            .filter(|e| {
                e.get("type").and_then(JsonValue::as_str) == Some("org.matrix.msc3401.call.member")
            })
            .map(|e| MemberStateEvent {
                event_id: e
                    .get("event_id")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                sender: e
                    .get("sender")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                origin_server_ts: e
                    .get("origin_server_ts")
                    .and_then(JsonValue::as_u64)
                    .unwrap_or_default(),
                state_key: e
                    .get("state_key")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                content: e.get("content").cloned().unwrap_or_default(),
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl RtcClientApi for HttpClientApi {
    async fn send_state_event(
        &self,
        room_id: &RoomId,
        event_type: StateEventType,
        state_key: &str,
        content: JsonValue,
    ) -> Result<SendEventResponse, ClientError> {
        let value = self
            .request(
                reqwest::Method::PUT,
                self.url(&format!(
                    "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}"
                )),
                &[],
                &content,
            )
            .await?;
        let event_id = value
            .get("event_id")
            .and_then(JsonValue::as_str)
            .unwrap_or("$unknown:example.org");
        Ok(SendEventResponse {
            event_id: OwnedEventId::try_from(event_id)
                .map_err(|e| ClientError::Other(e.to_string()))?,
        })
    }

    async fn send_delayed_state_event(
        &self,
        room_id: &RoomId,
        delay: Duration,
        event_type: StateEventType,
        state_key: &str,
        content: JsonValue,
    ) -> Result<SendDelayedEventResponse, ClientError> {
        let value = self
            .request(
                reqwest::Method::PUT,
                self.url(&format!(
                    "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}"
                )),
                &[(
                    "org.matrix.msc4140.delay".to_owned(),
                    delay.as_millis().to_string(),
                )],
                &content,
            )
            .await?;
        Ok(SendDelayedEventResponse {
            delay_id: value
                .get("delay_id")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_owned(),
        })
    }

    async fn update_delayed_event(
        &self,
        delay_id: &str,
        action: UpdateDelayedEventAction,
    ) -> Result<(), ClientError> {
        let action = match action {
            UpdateDelayedEventAction::Restart => "restart",
            UpdateDelayedEventAction::Cancel => "cancel",
            UpdateDelayedEventAction::Send => "send",
        };
        self.request(
            reqwest::Method::POST,
            self.url(&format!(
                "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}"
            )),
            &[],
            &json!({ "action": action }),
        )
        .await?;
        Ok(())
    }

    async fn send_event(
        &self,
        room_id: &RoomId,
        event_type: &str,
        content: JsonValue,
    ) -> Result<SendEventResponse, ClientError> {
        let value = self
            .request(
                reqwest::Method::PUT,
                self.url(&format!(
                    "/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{}",
                    Self::txn_id()
                )),
                &[],
                &content,
            )
            .await?;
        let event_id = value
            .get("event_id")
            .and_then(JsonValue::as_str)
            .unwrap_or("$unknown:example.org");
        Ok(SendEventResponse {
            event_id: OwnedEventId::try_from(event_id)
                .map_err(|e| ClientError::Other(e.to_string()))?,
        })
    }

    async fn encrypt_and_send_to_device(
        &self,
        event_type: &str,
        targets: &[ToDeviceTarget],
        content: JsonValue,
    ) -> Result<(), ClientError> {
        // DEVELOPMENT ONLY: sends the keys UNENCRYPTED. A real client must
        // encrypt with Olm via its crypto stack.
        let mut messages = serde_json::Map::new();
        for target in targets {
            messages
                .entry(target.user_id.clone())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("object")
                .insert(target.device_id.clone(), content.clone());
        }
        self.request(
            reqwest::Method::PUT,
            self.url(&format!(
                "/_matrix/client/v3/sendToDevice/{event_type}/{}",
                Self::txn_id()
            )),
            &[],
            &json!({ "messages": messages }),
        )
        .await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, homeserver, user_id, device_id, access_token, room_id] = args.as_slice() else {
        eprintln!("usage: join_call <homeserver> <user_id> <device_id> <access_token> <room_id>");
        exit(2);
    };

    let client = Arc::new(HttpClientApi {
        http: reqwest::Client::new(),
        homeserver: homeserver.clone(),
        access_token: access_token.clone(),
        user_id: user_id.clone(),
    });

    let own_identity = CallMembershipIdentity {
        user_id: user_id.clone(),
        device_id: device_id.clone(),
        member_id: format!("{user_id}:{device_id}"),
    };

    let session = RtcCallSession::new(
        Arc::clone(&client) as Arc<dyn RtcClientApi>,
        RtcRoom {
            room_id: room_id.as_str().try_into().expect("valid room id"),
            version: "default".to_owned(),
        },
        own_identity,
        RtcCallSessionConfig::default(),
    )
    .expect("valid own identity");
    let mut events = session.subscribe();

    // Initial call member state (one-shot; a real client feeds every sync).
    let member_events = client
        .get_call_member_events(room_id)
        .await
        .expect("fetch room state");
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap_or(u64::MAX);
    session.on_room_state_update(&member_events, |_| true, now);

    println!(
        "call members before join: {:?}",
        session
            .memberships()
            .iter()
            .map(|m| format!("{}:{}", m.user_id(), m.device_id()))
            .collect::<Vec<_>>()
    );

    // Join the MatrixRTC session (sends the delayed leave + membership
    // events in the background).
    session.join_rtc_session(Vec::new());

    // Resolve the focus: active focus of the session, if any.
    let service_url = session
        .get_active_focus()
        .and_then(|focus| focus.as_livekit().map(|lk| lk.service_url))
        .unwrap_or_else(|| {
            eprintln!("no active focus in the session; no LiveKit service to connect to");
            exit(1);
        });

    // Fetch the SFU JWT with our OpenID token (MSC4195).
    let openid_token = client.get_openid_token().await.expect("openid token");
    let sfu_config = fetch_sfu_config(
        &client.http,
        &service_url,
        room_id,
        device_id,
        &openid_token,
    )
    .await
    .expect("SFU config");
    println!("got SFU config: url={}", sfu_config.url);

    // Connect to LiveKit with E2EE enabled.
    let (connection, mut room_events) = LivekitCallConnection::connect(&sfu_config)
        .await
        .expect("LiveKit connect");
    println!("connected as {}", connection.local_identity());

    // Publish a silent microphone track.
    let audio_source = livekit::webrtc::audio_source::native::NativeAudioSource::new(
        livekit::webrtc::audio_source::AudioSourceOptions::default(),
        48_000,
        1,
        100,
    );
    connection
        .publish_microphone_track(livekit::webrtc::audio_source::RtcAudioSource::Native(
            audio_source,
        ))
        .await
        .expect("publish microphone");

    // Wire the E2EE keys of the session into the frame cryptor, and print
    // membership/track updates.
    loop {
        tokio::select! {
            Some(event) = events.recv() => match event {
                RtcCallSessionEvent::EncryptionKeyChanged {
                    key,
                    key_index,
                    rtc_backend_identity,
                    ..
                } => {
                    connection.set_participant_key(&rtc_backend_identity, key_index, key);
                    println!("set key {key_index} for {rtc_backend_identity}");
                }
                other => println!("session event: {other:?}"),
            },
            Some(event) = room_events.recv() => match event {
                livekit::RoomEvent::TrackSubscribed { track, participant, .. } => {
                    println!(
                        "subscribed to track {} of {}",
                        track.sid(),
                        participant.identity()
                    );
                }
                livekit::RoomEvent::Disconnected { reason } => {
                    println!("disconnected: {reason:?}");
                    break;
                }
                _ => {}
            },
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }
    }

    session
        .leave_rtc_session(Some(Duration::from_secs(10)))
        .await;
    let _ = connection.disconnect().await;
}
