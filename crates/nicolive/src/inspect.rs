use shiori_plugin::*;

use crate::program::NicoEmbeddedData;

pub struct NicoLiveInspector {
    user_session: Option<String>,
}

impl NicoLiveInspector {
    pub fn new(user_session: Option<String>) -> Self {
        NicoLiveInspector { user_session }
    }
}

#[async_trait]
impl Inspect for NicoLiveInspector {
    fn name(&self) -> String {
        "nicolive".to_string()
    }

    async fn matches(&self, url: &str) -> bool {
        url.starts_with("https://live.nicovideo.jp/watch/lv")
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        let data = NicoEmbeddedData::new(url, self.user_session.as_deref()).await?;
        let audience_token = data.audience_token()?;

        Ok(InspectResult::Playlist(InspectPlaylist {
            title: Some(data.program_title()),
            playlist_url: "dmc.nico".to_string(),
            playlist_type: PlaylistType::HLS,
            key: Some(audience_token),
            ..Default::default()
        }))
    }
}
