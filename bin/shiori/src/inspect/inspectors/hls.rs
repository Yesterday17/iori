use crate::inspect::{Inspect, InspectResult};
use clap_handler::async_trait;
use shiori_plugin::{InspectPlaylist, PlaylistType};

pub struct HlsInspector;

#[async_trait]
impl Inspect for HlsInspector {
    fn name(&self) -> String {
        "hls".to_string()
    }

    async fn matches(&self, url: &str) -> bool {
        url.contains(".m3u8")
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        Ok(InspectResult::Playlist(InspectPlaylist {
            playlist_url: url.to_string(),
            playlist_type: PlaylistType::HLS,
            ..Default::default()
        }))
    }
}
