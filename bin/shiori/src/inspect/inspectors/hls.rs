use crate::inspect::{Inspect, InspectResult};
use clap_handler::async_trait;
use shiori_plugin::*;

pub struct HlsInspector;

impl InspectorBuilder for HlsInspector {
    fn name(&self) -> String {
        "hls".to_string()
    }

    fn help(&self) -> Vec<String> {
        [
            "Downloads HLS playlists from the given URL.",
            "",
            "Requires the URL to contain '.m3u8'.",
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
impl Inspect for HlsInspector {
    async fn register(
        &self,
        id: InspectorIdentifier,
        registry: &mut InspectRegistry,
    ) -> anyhow::Result<()> {
        registry.register_http_route(
            RouterScheme::Both,
            "*".as_bytes(),
            "*.m3u8".as_bytes(),
            (
                id,
                Box::new(move |url, _| {
                    Box::pin(async move {
                        Ok(InspectResult::Playlist(InspectPlaylist {
                            playlist_url: url.to_string(),
                            playlist_type: PlaylistType::HLS,
                            ..Default::default()
                        }))
                    })
                }),
            ),
        )?;

        Ok(())
    }
}
