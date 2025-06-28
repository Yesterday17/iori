use std::str::FromStr;

use chrono::{DateTime, Utc};
use shiori_plugin::*;

use crate::{
    danmaku::{DanmakuClient, DanmakuList},
    model::{WatchMessageMessageServer, WatchMessageStream, WatchResponse},
    program::{NicoEmbeddedData, NivoServerResponse},
    watch::WatchClient,
};

pub struct NicoLiveInspector;

impl InspectorBuilder for NicoLiveInspector {
    fn name(&self) -> String {
        "nicolive".to_string()
    }

    fn help(&self) -> Vec<String> {
        [
            "Extracts Niconico live streams or timeshifts.",
            "",
            "Available for URLs starting with:",
            "- https://live.nicovideo.jp/watch/lv*",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn arguments(&self, command: &mut dyn InspectorCommand) {
        command.add_argument(
            "nico-user-session",
            Some("user_session"),
            "[Niconico] Your Niconico user session key.",
        );
        command.add_boolean_argument(
            "nico-download-danmaku",
            "[NicoLive] Download danmaku together with the video. This option is ignored if `--nico-danmaku-only` is set to true.",
        );
        command.add_boolean_argument(
            "nico-chase-play",
            "[NicoLive] Download an ongoing live from start.",
        );
        command.add_boolean_argument(
            "nico-reserve-timeshift",
            "[NicoLive] Whether to reserve a timeshift if not reserved.",
        );
        command.add_boolean_argument(
            "nico-danmaku-only",
            "[NicoLive] Only download danmaku without video.",
        );
    }

    fn build(&self, args: &dyn InspectorArguments) -> anyhow::Result<Box<dyn Inspect>> {
        let user_session = args.get_string("nico-user-session");
        let download_danmaku = args.get_boolean("nico-download-danmaku");
        let chase_play = args.get_boolean("nico-chase-play");
        let reserve_timeshift = args.get_boolean("nico-reserve-timeshift");
        let danmaku_only = args.get_boolean("nico-danmaku-only");
        Ok(Box::new(NicoLiveInspectorImpl {
            user_session,
            download_danmaku,
            chase_play,
            reserve_timeshift,
            danmaku_only,
        }))
    }
}

#[derive(Clone)]
struct NicoLiveInspectorImpl {
    user_session: Option<String>,
    download_danmaku: bool,
    chase_play: bool,
    reserve_timeshift: bool,
    danmaku_only: bool,
}

impl NicoLiveInspectorImpl {
    pub async fn download_danmaku(
        &self,
        message_server: WatchMessageMessageServer,
        program_end_time: u64,
    ) -> anyhow::Result<DanmakuList> {
        let client = DanmakuClient::new(message_server.view_uri).await?;
        let end_time = program_end_time + 30 * 60;
        let backward = client.get_backward_segment(end_time.to_string()).await?;
        let segment = backward
            .segment
            .ok_or_else(|| anyhow::anyhow!("No segment found in the backward segment"))?;
        let start_time = DateTime::<Utc>::from_str(&message_server.vpos_base_time)
            .map(|r| r.timestamp())
            .ok();

        let danmaku = client.recv_all(segment.uri, start_time).await?;
        Ok(danmaku)
    }
}

#[async_trait]
impl Inspect for NicoLiveInspectorImpl {
    async fn register(
        &self,
        id: InspectorIdentifier,
        registry: &mut InspectRegistry,
    ) -> anyhow::Result<()> {
        let this = self.clone();

        registry.register_http_route(
            RouterScheme::Https,
            "live.nicovideo.jp",
            "/watch/lv{id}",
            (
                id,
                Box::new(move |url, _| {
                    let this = this.clone();
                    Box::pin(async move {
                        let this: NicoLiveInspectorImpl = this.clone();
                        let data = NicoEmbeddedData::new(url.clone(), this.user_session.as_deref())
                            .await?;
                        let wss_url = if let Some(wss_url) = data.websocket_url() {
                            wss_url
                        } else if this.reserve_timeshift {
                            data.timeshift_reserve().await?;
                            let data =
                                NicoEmbeddedData::new(url, this.user_session.as_deref()).await?;
                            data.websocket_url()
                                .ok_or_else(|| anyhow::anyhow!("no websocket url"))?
                        } else {
                            anyhow::bail!("no websocket url");
                        };

                        let best_quality = data.best_quality()?;
                        let chase_play = this.chase_play;
                        let download_danmaku = this.download_danmaku || this.danmaku_only;

                        let watcher = WatchClient::new(&wss_url).await?;
                        watcher.start_watching(&best_quality, chase_play).await?;

                        let mut stream: Option<WatchMessageStream> = None;
                        let mut message_server: Option<WatchMessageMessageServer> = None;
                        loop {
                            let msg = watcher.recv().await?;
                            if let Some(WatchResponse::Stream(got_stream)) = msg {
                                stream = Some(got_stream);
                            } else if let Some(WatchResponse::MessageServer(got_message_server)) =
                                msg
                            {
                                message_server = Some(got_message_server);
                            }

                            if stream.is_some() && (!download_danmaku || message_server.is_some()) {
                                break;
                            }
                        }
                        let stream = stream.unwrap();

                        // keep seats
                        tokio::spawn(async move {
                            loop {
                                tokio::select! {
                                    msg = watcher.recv() => {
                                        if let Err(e) = msg {
                                            log::error!("{e:?}");
                                            if let Err(e) = watcher
                                                .reconnect(&wss_url, &best_quality, chase_play)
                                                .await
                                            {
                                                log::error!("Failed to reconnect: {e:?}");
                                                break;
                                            }
                                        }
                                    }
                                    _ = watcher.keep_seat() => (),
                                }
                            }
                            log::info!("watcher disconnected");
                        });

                        let mut result = vec![];
                        if !this.danmaku_only {
                            result.push(InspectPlaylist {
                                title: Some(data.program_title()),
                                playlist_url: stream.uri,
                                playlist_type: PlaylistType::HLS,
                                cookies: stream.cookies.into_cookies(),
                                streams_hint: Some(2),
                                ..Default::default()
                            });
                        }

                        if let Some(message_server) = message_server {
                            let danmaku = this
                                .download_danmaku(message_server, data.program_end_time())
                                .await?;
                            result.push(InspectPlaylist {
                                title: Some(data.program_title()),
                                playlist_url: danmaku.to_json(true)?,
                                playlist_type: PlaylistType::Raw("json".to_string()),
                                ..Default::default()
                            });
                            result.push(InspectPlaylist {
                                title: Some(data.program_title()),
                                playlist_url: danmaku.to_ass()?,
                                playlist_type: PlaylistType::Raw("ass".to_string()),
                                ..Default::default()
                            });
                        }

                        Ok(InspectResult::Playlists(result))
                    })
                }),
            ),
        )?;

        Ok(())
    }
}

pub struct NicoVideoInspector;

impl InspectorBuilder for NicoVideoInspector {
    fn name(&self) -> String {
        "nicovideo".to_string()
    }

    fn help(&self) -> Vec<String> {
        [
            "Extracts Niconico videos.",
            "",
            "Available for URLs starting with:",
            "- https://www.nicovideo.jp/watch/so*",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn arguments(&self, command: &mut dyn InspectorCommand) {
        command.add_argument(
            "nico-user-session",
            Some("user_session"),
            "[Niconico] Your Niconico user session key.",
        );
    }

    fn build(&self, args: &dyn InspectorArguments) -> anyhow::Result<Box<dyn Inspect>> {
        let user_session = args.get_string("nico-user-session");
        Ok(Box::new(NicoVideoInspectorImpl { user_session }))
    }
}

struct NicoVideoInspectorImpl {
    user_session: Option<String>,
}

#[async_trait]
impl Inspect for NicoVideoInspectorImpl {
    async fn register(
        &self,
        id: InspectorIdentifier,
        registry: &mut InspectRegistry,
    ) -> anyhow::Result<()> {
        let user_session = self.user_session.clone();
        registry.register_http_route(
            RouterScheme::Https,
            "www.nicovideo.jp",
            "/watch/so{id}",
            (
                id,
                Box::new(move |url, _| {
                    let user_session = user_session.clone();
                    Box::pin(async move {
                        let data = NivoServerResponse::new(url, user_session.as_deref()).await?;
                        let (playlist_url, cookies) = data.playlist_url().await?;
                        Ok(InspectResult::Playlists(vec![InspectPlaylist {
                            title: data.program_title(),
                            playlist_url,
                            playlist_type: PlaylistType::HLS,
                            headers: vec![format!("Cookie: {cookies}")],
                            ..Default::default()
                        }]))
                    })
                }),
            ),
        )?;

        Ok(())
    }
}
