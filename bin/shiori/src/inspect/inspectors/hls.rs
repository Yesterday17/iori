use crate::inspect::{Inspect, InspectResult};
use clap_handler::async_trait;
use shiori_plugin::{InspectPlaylist, InspectorBuilder, PlaylistType};

pub struct HlsInspector;

impl InspectorBuilder for HlsInspector {
    fn name(&self) -> String {
        "hls".to_string()
    }

    fn build(&self, _args: &shiori_plugin::InspectorArgs) -> anyhow::Result<Box<dyn Inspect>> {
        Ok(Box::new(HlsInspectorImpl))
    }
}

struct HlsInspectorImpl;

#[async_trait]
impl Inspect for HlsInspectorImpl {
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
