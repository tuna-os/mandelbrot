//! Media layer for native calls, bridging the `MatrixRTC` engine to the
//! `LiveKit` SFU.
//!
//! Only compiled with the `calls-media` feature. The media task runs on the
//! tokio runtime: it fetches the SFU JWT with our `OpenID` token, connects to
//! the `LiveKit` room with end-to-end encryption, publishes the microphone
//! (captured with `GStreamer`) and forwards remote video frames to the UI.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gst::prelude::*;
use mandelbrot_matrixrtc::{
    RtcCallSession, RtcCallSessionEvent, Transport, livekit,
    livekit_connection::{LivekitCallConnection, OpenIdToken, SfuConfig, fetch_sfu_config},
};
use matrix_sdk::Client;
use ruma::{OwnedRoomId, api::client::account::request_openid_token};
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{debug, error, warn};

/// The sample rate of the published microphone track.
const MIC_SAMPLE_RATE: u32 = 48_000;
/// The number of channels of the published microphone track.
const MIC_CHANNELS: u32 = 1;

/// An event from the media task, consumed on the main context.
pub(super) enum MediaEvent {
    /// The connection to the SFU is established.
    Connected,
    /// A video frame of a remote participant arrived.
    VideoFrame {
        /// The `LiveKit` identity of the participant.
        identity: String,
        /// The frame, as RGBA bytes.
        rgba: Vec<u8>,
        /// The width of the frame.
        width: u32,
        /// The height of the frame.
        height: u32,
    },
    /// The video track of a remote participant went away.
    VideoEnded {
        /// The `LiveKit` identity of the participant.
        identity: String,
    },
    /// The media connection failed or ended.
    Ended {
        /// A human-readable reason, if the connection failed.
        error: Option<String>,
    },
}

/// A handle to the media connection of one call.
pub(super) struct MediaHandle {
    task: JoinHandle<()>,
    /// Whether the microphone is muted.
    muted: Arc<AtomicBool>,
}

impl MediaHandle {
    /// Set whether the microphone is muted.
    pub(super) fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::SeqCst);
    }

    /// The flag controlling whether the microphone is muted.
    pub(super) fn muted_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.muted)
    }
}

impl Drop for MediaHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Start the media connection for the given call.
///
/// Must be called from within the tokio runtime context. Events for the UI
/// are delivered on the returned channel; the connection is torn down when
/// the handle is dropped.
pub(super) fn start(
    client: Client,
    room_id: OwnedRoomId,
    device_id: String,
    engine: Arc<RtcCallSession>,
    preferred_foci: Vec<Transport>,
) -> (MediaHandle, mpsc::Receiver<MediaEvent>) {
    let (tx, rx) = mpsc::channel(8);
    let muted = Arc::new(AtomicBool::new(false));

    let task_muted = Arc::clone(&muted);
    // Must go through the shared runtime: this is called from a GTK signal
    // handler on the main thread, where `tokio::spawn` has no reactor and
    // aborts the process.
    let task = spawn_tokio!(async move {
        let error = match run(
            client,
            room_id,
            device_id,
            engine,
            preferred_foci,
            &tx,
            task_muted,
        )
        .await
        {
            Ok(()) => None,
            Err(error) => {
                error!("Call media connection failed: {error}");
                Some(error)
            }
        };
        let _ = tx.send(MediaEvent::Ended { error }).await;
    });

    (MediaHandle { task, muted }, rx)
}

/// Fetch a Matrix `OpenID` token for our own user.
async fn get_openid_token(client: &Client) -> Result<OpenIdToken, String> {
    let user_id = client
        .user_id()
        .ok_or_else(|| "not logged in".to_owned())?
        .to_owned();

    let response = client
        .send(request_openid_token::v3::Request::new(user_id))
        .await
        .map_err(|error| format!("failed to fetch OpenID token: {error}"))?;

    Ok(OpenIdToken {
        access_token: response.access_token,
        token_type: response.token_type.to_string(),
        matrix_server_name: response.matrix_server_name.to_string(),
        expires_in: response.expires_in.as_secs(),
    })
}

/// The `LiveKit` service URL to use for the call.
fn service_url(engine: &RtcCallSession, preferred_foci: &[Transport]) -> Option<String> {
    engine
        .get_active_focus()
        .as_ref()
        .and_then(Transport::as_livekit)
        .map(|focus| focus.service_url)
        .or_else(|| {
            preferred_foci
                .iter()
                .find_map(|focus| focus.as_livekit().map(|focus| focus.service_url))
        })
}

/// Run the media connection until the task is aborted or the room
/// disconnects.
async fn run(
    client: Client,
    room_id: OwnedRoomId,
    device_id: String,
    engine: Arc<RtcCallSession>,
    preferred_foci: Vec<Transport>,
    tx: &mpsc::Sender<MediaEvent>,
    muted: Arc<AtomicBool>,
) -> Result<(), String> {
    // Subscribe before connecting so that no key event is lost.
    let mut engine_events = engine.subscribe();

    let service_url = service_url(&engine, &preferred_foci)
        .ok_or_else(|| "no LiveKit focus available for this call".to_owned())?;

    let openid_token = get_openid_token(&client).await?;
    let http = mandelbrot_matrixrtc::reqwest::Client::new();
    let sfu_config: SfuConfig = fetch_sfu_config(
        &http,
        &service_url,
        room_id.as_str(),
        &device_id,
        &openid_token,
    )
    .await
    .map_err(|error| format!("failed to fetch the SFU configuration: {error}"))?;

    let (connection, mut room_events) = Box::pin(LivekitCallConnection::connect(&sfu_config))
        .await
        .map_err(|error| format!("failed to connect to the SFU: {error}"))?;
    debug!("Connected to LiveKit as {}", connection.local_identity());

    // Keys that arrived before the connection was established.
    if let Some(key_rings) = engine.get_encryption_keys() {
        for ring in key_rings.values() {
            connection.apply_key_ring(ring.iter());
        }
    }

    let _ = tx.send(MediaEvent::Connected).await;

    // Publish the microphone.
    let mic = publish_microphone(&connection, muted).await;
    let _mic_guard = match mic {
        Ok(guard) => Some(guard),
        Err(error) => {
            // A call without a microphone is still useful to listen in.
            warn!("Failed to publish the microphone: {error}");
            None
        }
    };

    let mut video_tasks: Vec<JoinHandle<()>> = Vec::new();
    let result = loop {
        tokio::select! {
            event = engine_events.recv() => {
                let Some(event) = event else {
                    break Ok(());
                };
                if let RtcCallSessionEvent::EncryptionKeyChanged {
                    key,
                    key_index,
                    rtc_backend_identity,
                    ..
                } = event
                {
                    connection.set_participant_key(&rtc_backend_identity, key_index, key);
                }
            }
            event = room_events.recv() => {
                let Some(event) = event else {
                    break Ok(());
                };
                match event {
                    livekit::RoomEvent::TrackSubscribed {
                        track: livekit::track::RemoteTrack::Video(video),
                        participant,
                        ..
                    } => {
                        video_tasks.push(spawn_video_task(
                            &video,
                            participant.identity().to_string(),
                            tx.clone(),
                        ));
                    }
                    livekit::RoomEvent::TrackUnsubscribed {
                        track: livekit::track::RemoteTrack::Video(_),
                        participant,
                        ..
                    } => {
                        let _ = tx
                            .send(MediaEvent::VideoEnded {
                                identity: participant.identity().to_string(),
                            })
                            .await;
                    }
                    livekit::RoomEvent::Disconnected { reason } => {
                        debug!("Disconnected from LiveKit: {reason:?}");
                        break Ok(());
                    }
                    _ => {}
                }
            }
        }
    };

    for task in video_tasks {
        task.abort();
    }
    let _ = connection.disconnect().await;
    result
}

/// A guard stopping the microphone pipeline on drop.
struct MicrophoneGuard {
    pipeline: gst::Pipeline,
    pump: JoinHandle<()>,
}

impl Drop for MicrophoneGuard {
    fn drop(&mut self) {
        self.pump.abort();
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Capture the default microphone with `GStreamer` and publish it as an audio
/// track.
async fn publish_microphone(
    connection: &LivekitCallConnection,
    muted: Arc<AtomicBool>,
) -> Result<MicrophoneGuard, String> {
    use livekit::webrtc::{
        audio_frame::AudioFrame,
        audio_source::{RtcAudioSource, native::NativeAudioSource},
    };

    let source = NativeAudioSource::new(
        livekit::webrtc::audio_source::AudioSourceOptions {
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain_control: true,
        },
        MIC_SAMPLE_RATE,
        MIC_CHANNELS,
        1000,
    );

    let pipeline = gst::parse::launch(&format!(
        "autoaudiosrc ! audioconvert ! audioresample ! \
         audio/x-raw,format=S16LE,rate={MIC_SAMPLE_RATE},channels={MIC_CHANNELS} ! \
         appsink name=sink emit-signals=false sync=false"
    ))
    .map_err(|error| format!("failed to create the microphone pipeline: {error}"))?
    .downcast::<gst::Pipeline>()
    .map_err(|_| "microphone pipeline is not a pipeline".to_owned())?;

    let appsink = pipeline
        .by_name("sink")
        .and_then(|sink| sink.downcast::<gst_app::AppSink>().ok())
        .ok_or_else(|| "no appsink in the microphone pipeline".to_owned())?;

    let (samples_tx, mut samples_rx) = mpsc::channel::<Vec<i16>>(16);
    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |appsink| {
                let Ok(sample) = appsink.pull_sample() else {
                    return Err(gst::FlowError::Eos);
                };
                if muted.load(Ordering::SeqCst) {
                    return Ok(gst::FlowSuccess::Ok);
                }
                let Some(buffer) = sample.buffer() else {
                    return Ok(gst::FlowSuccess::Ok);
                };
                let Ok(map) = buffer.map_readable() else {
                    return Ok(gst::FlowSuccess::Ok);
                };
                let bytes = map.as_slice();
                let mut samples = vec![0i16; bytes.len() / 2];
                for (sample, chunk) in samples.iter_mut().zip(bytes.chunks_exact(2)) {
                    *sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                }
                // Drop frames if the pump cannot keep up.
                let _ = samples_tx.try_send(samples);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    pipeline
        .set_state(gst::State::Playing)
        .map_err(|error| format!("failed to start the microphone pipeline: {error}"))?;

    connection
        .publish_microphone_track(RtcAudioSource::Native(source.clone()))
        .await
        .map_err(|error| format!("failed to publish the microphone track: {error}"))?;

    let pump = tokio::spawn(async move {
        while let Some(samples) = samples_rx.recv().await {
            let samples_per_channel =
                u32::try_from(samples.len()).unwrap_or(u32::MAX) / MIC_CHANNELS;
            let frame = AudioFrame {
                data: samples.into(),
                sample_rate: MIC_SAMPLE_RATE,
                num_channels: MIC_CHANNELS,
                samples_per_channel,
            };
            if let Err(error) = source.capture_frame(&frame).await {
                warn!("Failed to capture a microphone frame: {error}");
            }
        }
    });

    Ok(MicrophoneGuard { pipeline, pump })
}

/// Spawn a task converting the frames of a remote video track to RGBA and
/// forwarding them to the UI.
fn spawn_video_task(
    video: &livekit::track::RemoteVideoTrack,
    identity: String,
    tx: mpsc::Sender<MediaEvent>,
) -> JoinHandle<()> {
    use futures_util::StreamExt;
    use livekit::webrtc::{
        native::yuv_helper, video_frame::VideoBuffer, video_stream::native::NativeVideoStream,
    };

    let mut stream = NativeVideoStream::new(video.rtc_track());
    tokio::spawn(async move {
        while let Some(frame) = stream.next().await {
            let buffer = frame.buffer.to_i420();
            let width = buffer.width();
            let height = buffer.height();
            let (stride_y, stride_u, stride_v) = buffer.strides();
            let (data_y, data_u, data_v) = buffer.data();

            let mut rgba = vec![0u8; (width * height * 4) as usize];
            yuv_helper::i420_to_abgr(
                data_y,
                stride_y,
                data_u,
                stride_u,
                data_v,
                stride_v,
                &mut rgba,
                width * 4,
                i32::try_from(width).unwrap_or(i32::MAX),
                i32::try_from(height).unwrap_or(i32::MAX),
            );

            // Drop frames if the UI cannot keep up.
            let event = MediaEvent::VideoFrame {
                identity: identity.clone(),
                rgba,
                width,
                height,
            };
            if tx.try_send(event).is_err() && tx.is_closed() {
                break;
            }
        }
        let _ = tx.send(MediaEvent::VideoEnded { identity }).await;
    })
}
