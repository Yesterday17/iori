use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::{REFERER, USER_AGENT};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::{Mutex, Notify};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::ice::network_type::NetworkType;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::interceptor::registry::Registry;
use webrtc::media::io::h264_writer::H264Writer;
use webrtc::media::io::ogg_writer::OggWriter;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::policy::bundle_policy::RTCBundlePolicy;
use webrtc::peer_connection::policy::rtcp_mux_policy::RTCRtcpMuxPolicy;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::{RTCPFeedback, RTCRtpTransceiverInit};
use webrtc::track::track_remote::TrackRemote;
use writer::AsyncBufferWriter;

use crate::{IoriResult, StreamingSource};
use crate::{SegmentFormat, SegmentType, StreamingSegment};

mod writer;

pub struct WebRTCLiveSource {}

async fn save_to_disk(
    writer: Arc<Mutex<dyn webrtc::media::io::Writer + Send + Sync>>,
    track: Arc<TrackRemote>,
    notify: Arc<Notify>,
) -> IoriResult<()> {
    loop {
        tokio::select! {
            result = track.read_rtp() => {
                match result {
                    Ok((rtp_packet, _)) => {
                        let mut w = writer.lock().await;
                        _ = w.write_rtp(&rtp_packet).inspect_err(|e|log::error!("writer error: {e}, len={}, {:?}", rtp_packet.payload.len(), rtp_packet.payload));

                    }
                    Err(e) => {
                        log::debug!("file closing begin after read_rtp error: {e}");
                        let mut w = writer.lock().await;
                        _ = w.close().inspect_err(|e| log::error!("file close err: {e}"));
                        log::debug!("file closing end after read_rtp error");
                        return Ok(());
                    }
                }
            }
            _ = notify.notified() => {
                log::debug!("file closing begin after notified");
                let mut w = writer.lock().await;
                _ = w.close().inspect_err(|e| log::error!("file close err: {e}"));
                log::debug!("file closing end after notified");
                return Ok(());
            }
        }
    }
}

impl WebRTCLiveSource {
    fn media_engine() -> IoriResult<MediaEngine> {
        // Create a MediaEngine object to configure the supported codec
        let mut m = MediaEngine::default();

        // m.register_default_codecs();
        m.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;stereo=1;useinbandfec=1".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 111,
                ..Default::default()
            },
            RTPCodecType::Audio,
        )?;

        let video_rtcp_feedback = vec![
            RTCPFeedback {
                typ: "goog-remb".to_owned(),
                parameter: "".to_owned(),
            },
            RTCPFeedback {
                typ: "ccm".to_owned(),
                parameter: "fir".to_owned(),
            },
            RTCPFeedback {
                typ: "nack".to_owned(),
                parameter: "".to_owned(),
            },
            RTCPFeedback {
                typ: "nack".to_owned(),
                parameter: "pli".to_owned(),
            },
        ];
        for codec in vec![
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 102,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 127,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 125,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 108,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 127,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 123,
                ..Default::default()
            },
        ] {
            m.register_codec(codec, RTPCodecType::Video)?;
        }

        Ok(m)
    }

    async fn doit(tx: mpsc::UnboundedSender<IoriResult<Vec<WebRTCSegment>>>) -> IoriResult<()> {
        let (h264_tx, mut h264_rx) = mpsc::channel(16);
        let (ogg_tx, mut ogg_rx) = mpsc::channel(16);

        let tx1 = tx.clone();
        tokio::spawn(async move {
            let seq = AtomicU64::new(0);
            while let Some(chunk) = h264_rx.recv().await {
                let bytes = Bytes::from_owner(chunk);
                _ = tx1.send(Ok(vec![WebRTCSegment::new(
                    bytes,
                    true,
                    seq.fetch_add(1, Ordering::Relaxed),
                )]));
            }
        });
        tokio::spawn(async move {
            let seq = AtomicU64::new(0);
            while let Some(chunk) = ogg_rx.recv().await {
                let bytes = Bytes::from_owner(chunk);
                _ = tx.send(Ok(vec![WebRTCSegment::new(
                    bytes,
                    false,
                    seq.fetch_add(1, Ordering::Relaxed),
                )]));
            }
        });

        let h264_writer: Arc<Mutex<dyn webrtc::media::io::Writer + Send + Sync>> =
            Arc::new(Mutex::new(H264Writer::new(AsyncBufferWriter::new(
                2 * 1024 * 1024,
                h264_tx,
            ))));
        let ogg_writer: Arc<Mutex<dyn webrtc::media::io::Writer + Send + Sync>> =
            Arc::new(Mutex::new(OggWriter::new(
                AsyncBufferWriter::new(2 * 1024 * 1024, ogg_tx),
                48000,
                2,
            )?));

        // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

        // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
        // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
        // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
        // for each PeerConnection.
        let mut registry = Registry::new();
        let mut media_engine = Self::media_engine()?;

        // Use the default set of Interceptors
        registry = register_default_interceptors(registry, &mut media_engine)?;

        let mut settings = SettingEngine::default();
        settings.set_ice_timeouts(
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(30)),
            Some(Duration::from_millis(2000)),
        );
        settings.set_network_types(vec![NetworkType::Udp4]);

        // Create the API object with the MediaEngine
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_setting_engine(settings)
            .build();

        // Prepare the configuration
        let config = RTCConfiguration {
            ice_servers: vec![],
            rtcp_mux_policy: RTCRtcpMuxPolicy::Require,
            bundle_policy: RTCBundlePolicy::MaxBundle,
            ..Default::default()
        };

        // Create a new RTCPeerConnection
        let peer_connection = Arc::new(api.new_peer_connection(config).await?);

        // Allow us to receive 1 audio track, and 1 video track
        peer_connection
            .add_transceiver_from_kind(
                RTPCodecType::Audio,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    send_encodings: vec![],
                }),
            )
            .await?;
        peer_connection
            .add_transceiver_from_kind(
                RTPCodecType::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    send_encodings: vec![],
                }),
            )
            .await?;

        let notify_tx = Arc::new(Notify::new());
        let notify_rx = notify_tx.clone();

        // Set a handler for when a new remote track starts, this handler saves buffers to disk as
        // an ivf file, since we could have multiple video tracks we provide a counter.
        // In your application this is where you would handle/process video
        peer_connection.on_track(Box::new(move |track, _, _| {
            let notify_rx2 = Arc::clone(&notify_rx);
            let h264_writer2 = Arc::clone(&h264_writer);
            let ogg_writer2 = Arc::clone(&ogg_writer);
            Box::pin(async move {
                let codec = track.codec();
                let mime_type = codec.capability.mime_type.to_lowercase();
                if mime_type == MIME_TYPE_OPUS.to_lowercase() {
                    log::debug!(
                        "Got Opus track, saving to disk as output.opus (48 kHz, 2 channels)"
                    );
                    tokio::spawn(async move {
                        _ = save_to_disk(ogg_writer2, track, notify_rx2)
                            .await
                            .inspect_err(|e| log::error!("opus error: {e}"));
                    });
                } else if mime_type == MIME_TYPE_H264.to_lowercase() {
                    log::debug!("Got h264 track, saving to disk as output.h264");
                    tokio::spawn(async move {
                        _ = save_to_disk(h264_writer2, track, notify_rx2)
                            .await
                            .inspect_err(|e| log::error!("h264 error: {e}"));
                    });
                } else {
                    log::warn!("Got unknown track {}", mime_type);
                }
            })
        }));

        let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Set the handler for ICE connection state
        // This will notify you when the peer has connected/disconnected
        peer_connection.on_ice_connection_state_change(Box::new(
            move |connection_state: RTCIceConnectionState| {
                log::debug!("Connection State has changed {connection_state}");

                if connection_state == RTCIceConnectionState::Failed {
                    notify_tx.notify_waiters();

                    log::debug!("Done writing media files");

                    let _ = done_tx.try_send(());
                }

                Box::pin(async {})
            },
        ));

        // Output the answer in base64 so we can paste it in browser
        let offer = peer_connection.create_offer(None).await?;
        let remote_offer = Self::signaling(&offer).await?;
        peer_connection.set_local_description(offer).await?;
        peer_connection.set_remote_description(remote_offer).await?;

        // Create channel that is blocked until ICE Gathering is complete
        let mut gather_complete = peer_connection.gathering_complete_promise().await;

        // Block until ICE Gathering is complete, disabling trickle ICE
        // we do this because we only can exchange one signaling message
        // in a production application you should exchange ICE Candidates via OnICECandidate
        let _ = gather_complete.recv().await;

        done_rx.recv().await;
        peer_connection.close().await?;

        Ok(())
    }

    async fn signaling(offer: &RTCSessionDescription) -> IoriResult<RTCSessionDescription> {
        todo!()
    }
}

impl StreamingSource for WebRTCLiveSource {
    type Segment = WebRTCSegment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move { Self::doit(tx).await });

        Ok(rx)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        writer.write_all(segment.data.as_ref()).await?;
        Ok(())
    }
}

pub struct WebRTCSegment {
    is_video: bool,
    sequence: u64,
    file_name: String,

    data: Bytes,
}

impl WebRTCSegment {
    fn new(data: Bytes, is_video: bool, sequence: u64) -> Self {
        WebRTCSegment {
            data,
            sequence,
            is_video,
            file_name: format!("data.{}", if is_video { "h264" } else { "ogg" }),
        }
    }
}

impl StreamingSegment for WebRTCSegment {
    fn stream_id(&self) -> u64 {
        if self.is_video {
            0
        } else {
            1
        }
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }

    fn key(&self) -> Option<std::sync::Arc<crate::decrypt::IoriKey>> {
        None
    }

    fn r#type(&self) -> SegmentType {
        if self.is_video {
            SegmentType::Video
        } else {
            SegmentType::Audio
        }
    }

    fn format(&self) -> SegmentFormat {
        if self.is_video {
            SegmentFormat::Raw("h264".to_string())
        } else {
            SegmentFormat::Raw("ogg".to_string())
        }
    }
}
