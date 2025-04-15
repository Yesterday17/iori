use shiori_plugin::*;

use crate::{model::WatchResponse, program::NicoEmbeddedData, watch::WatchClient};

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
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn build(&self, args: &InspectorArgs) -> anyhow::Result<Box<dyn Inspect>> {
        let key = args.get("nico_user_session");
        Ok(Box::new(NicoLiveInspectorImpl::new(key)))
    }
}

struct NicoLiveInspectorImpl {
    user_session: Option<String>,
}

impl NicoLiveInspectorImpl {
    pub fn new(user_session: Option<String>) -> Self {
        Self { user_session }
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
        watcher.start_watching(&best_quality).await?;

        let stream = loop {
            let msg = watcher.recv().await?;
            if let Some(WatchResponse::Stream(stream)) = msg {
                break stream;
            }
        };

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

        Ok(InspectResult::Playlist(InspectPlaylist {
            title: Some(data.program_title()),
            playlist_url: stream.uri,
            playlist_type: PlaylistType::HLS,
            cookies: stream.cookies.into_cookies(),
            ..Default::default()
        }))
    }
}
