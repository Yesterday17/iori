use std::str::FromStr;

use chrono::{DateTime, Utc};
use shiori_plugin::*;

use crate::{
    danmaku::{DanmakuList, NewDanmakuClient},
    model::{WatchMessageMessageServer, WatchMessageStream, WatchResponse},
    program::NicoEmbeddedData,
    watch::WatchClient,
};

pub struct NicoLiveInspector;

impl InspectorBuilder for NicoLiveInspector {
    fn name(&self) -> String {
        "nicolive".to_string()
    }

    fn help(&self) -> Vec<String> {
        [
            "Extracts NicoLive live streams or timeshifts.",
            "",
            "Available for URLs starting with:",
            "- https://live.nicovideo.jp/watch/lv",
            "",
            "Arguments:",
            "- nico_user_session: Your NicoLive user session key.",
            "- nico_danmaku: Whether to download danmaku together with the video. (yes/no, default: no)",
            "- nico_chase_play: Whether to chase play the video. (yes/no, default: no)",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn build(&self, args: &InspectorArgs) -> anyhow::Result<Box<dyn Inspect>> {
        let key = args.get("nico_user_session");
        let download_danmaku = args
            .get("nico_danmaku")
            .map(|d| d == "yes")
            .unwrap_or(false);
        let chase_play = args
            .get("nico_chase_play")
            .map(|d| d == "yes")
            .unwrap_or(false);
        Ok(Box::new(NicoLiveInspectorImpl::new(
            key,
            download_danmaku,
            chase_play,
        )))
    }
}

struct NicoLiveInspectorImpl {
    user_session: Option<String>,
    download_danmaku: bool,
    chase_play: bool,
}

impl NicoLiveInspectorImpl {
    pub fn new(user_session: Option<String>, download_danmaku: bool, chase_play: bool) -> Self {
        Self {
            user_session,
            download_danmaku,
            chase_play,
        }
    }

    pub async fn download_danmaku(
        &self,
        message_server: WatchMessageMessageServer,
        program_end_time: u64,
    ) -> anyhow::Result<DanmakuList> {
        let client = NewDanmakuClient::new(message_server.view_uri).await?;
        let end_time = program_end_time + 30 * 60;
        let backward = client.get_backward_segment(end_time.to_string()).await?;
        let segment = backward.segment.unwrap();
        let start_time = DateTime::<Utc>::from_str(&message_server.vpos_base_time)
            .map(|r| r.timestamp())
            .ok();

        let danmaku = client.recv_all(segment.uri, start_time).await?;
        Ok(danmaku)
    }
}

#[async_trait]
impl Inspect for NicoLiveInspectorImpl {
    async fn matches(&self, url: &str) -> bool {
        url.starts_with("https://live.nicovideo.jp/watch/lv")
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        let data = NicoEmbeddedData::new(url, self.user_session.as_deref()).await?;
        let wss_url = data
            .websocket_url()
            .ok_or_else(|| anyhow::anyhow!("no websocket url"))?;
        let best_quality = data.best_quality()?;

        let watcher = WatchClient::new(&wss_url).await?;
        watcher
            .start_watching(&best_quality, self.chase_play)
            .await?;

        let mut stream: Option<WatchMessageStream> = None;
        let mut message_server: Option<WatchMessageMessageServer> = None;
        loop {
            let msg = watcher.recv().await?;
            if let Some(WatchResponse::Stream(got_stream)) = msg {
                stream = Some(got_stream);
            } else if let Some(WatchResponse::MessageServer(got_message_server)) = msg {
                message_server = Some(got_message_server);
            }

            if stream.is_some() && (!self.download_danmaku || message_server.is_some()) {
                break;
            }
        }
        let stream = stream.unwrap();

        // keep seats
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = watcher.recv() => {
                        let Ok(msg) = msg else {
                            break;
                        };
                        log::debug!("message: {:?}", msg);
                    }
                    _ = watcher.keep_seat() => (),
                }
            }
            log::info!("watcher disconnected");
        });

        let mut result = vec![InspectPlaylist {
            title: Some(data.program_title()),
            playlist_url: stream.uri,
            playlist_type: PlaylistType::HLS,
            cookies: stream.cookies.into_cookies(),
            ..Default::default()
        }];
        if let Some(message_server) = message_server {
            let danmaku = self
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
    }
}
