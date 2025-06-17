use crate::inspect::{Inspect, InspectResult};
use clap_handler::async_trait;
use iori::PlaylistType;
use shiori_plugin::{InspectPlaylist, InspectorArguments, InspectorBuilder};

pub struct DashInspector;

impl InspectorBuilder for DashInspector {
    fn name(&self) -> String {
        "dash".to_string()
    }

    fn help(&self) -> Vec<String> {
        [
            "Downloads MPEG-DASH manifests from the given URL.",
            "",
            "Requires the URL to contain '.mpd'.",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn build(&self, _args: &dyn InspectorArguments) -> anyhow::Result<Box<dyn Inspect>> {
        Ok(Box::new(Self))
    }
}

#[async_trait]
impl Inspect for DashInspector {
    async fn matches(&self, url: &str) -> bool {
        url.contains(".mpd")
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        Ok(InspectResult::Playlist(InspectPlaylist {
            playlist_url: url.to_string(),
            playlist_type: PlaylistType::DASH,
            ..Default::default()
        }))
    }
}
