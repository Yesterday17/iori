use reqwest::Url;
use shiori_plugin::*;

use crate::ShowRoomClient;

pub struct ShowroomInspector;

#[async_trait]
impl Inspect for ShowroomInspector {
    fn name(&self) -> &'static str {
        "showroom"
    }

    async fn matches(&self, url: &str) -> bool {
        url.starts_with("https://www.showroom-live.com/r/")
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        let url = Url::parse(url)?;
        let room_name = url.path().trim_start_matches("/r/");

        let client = ShowRoomClient::default();
        let room_id = match room_name.parse::<u64>() {
            Ok(room_id) => room_id,
            Err(_) => client.get_id_by_room_name(room_name).await?,
        };

        let info = client.live_info(room_id).await?;
        if !info.is_living() {
            return Ok(InspectResult::None);
        }

        let streams = client.streaming_url(room_id).await?;
        let stream = streams.best(false);

        Ok(InspectResult::Playlist(InspectPlaylist {
            title: Some(info.room_name),
            playlist_url: stream.url.clone(),
            playlist_type: PlaylistType::HLS,
            ..Default::default()
        }))
    }
}
