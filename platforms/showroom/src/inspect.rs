use reqwest::Url;
use shiori_plugin::*;

use crate::ShowRoomClient;

pub struct ShowroomInspector;

impl InspectorBuilder for ShowroomInspector {
    fn name(&self) -> String {
        "showroom".to_string()
    }

    fn help(&self) -> Vec<String> {
        [
            "Extracts Showroom playlists from the given URL.",
            "",
            "Template:",
            "- https://www.showroom-live.com/r/*",
            "- https://www.showroom-live.com/timeshift/*",
            "",
            "Arguments:",
            "- sr_id: Your Showroom user session key.",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn build(&self, args: &InspectorArgs) -> anyhow::Result<Box<dyn Inspect>> {
        Ok(Box::new(ShowroomInspectorImpl(args.get("sr_id"))))
    }
}

struct ShowroomInspectorImpl(Option<String>);

impl ShowroomInspectorImpl {
    fn is_timeshift_url(&self, url: &str) -> bool {
        let re = regex::Regex::new(r"^https://www\.showroom-live\.com/timeshift/([^/]+)/([^/]+)$")
            .unwrap();
        re.is_match(url)
    }

    fn extract_timeshift_keys(&self, url: &str) -> Option<(String, String)> {
        let re = regex::Regex::new(r"^https://www\.showroom-live\.com/timeshift/([^/]+)/([^/]+)$")
            .unwrap();
        re.captures(url).map(|caps| {
            let room_url_key = caps.get(1).unwrap().as_str().to_string();
            let view_url_key = caps.get(2).unwrap().as_str().to_string();
            (room_url_key, view_url_key)
        })
    }
}

#[async_trait]
impl Inspect for ShowroomInspectorImpl {
    async fn matches(&self, url: &str) -> bool {
        url.starts_with("https://www.showroom-live.com/r/")
            || url.starts_with("https://www.showroom-live.com/timeshift/")
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        let client = ShowRoomClient::new(self.0.clone());

        if self.is_timeshift_url(url) {
            let (room_url_key, view_url_key) = self.extract_timeshift_keys(url).unwrap();
            let timeshift_info = client.timeshift_info(&room_url_key, &view_url_key).await?;
            let timeshift_streaming_url = client
                .timeshift_streaming_url(
                    timeshift_info.timeshift.room_id,
                    timeshift_info.timeshift.live_id,
                )
                .await?;
            let stream = timeshift_streaming_url.best();
            return Ok(InspectResult::Playlist(InspectPlaylist {
                title: Some(timeshift_info.timeshift.title),
                playlist_url: stream.url().to_string(),
                playlist_type: PlaylistType::HLS,
                ..Default::default()
            }));
        } else {
            // live
            let url: Url = Url::parse(url)?;
            let room_name = url.path().trim_start_matches("/r/");

            let room_id = match room_name.parse::<u64>() {
                Ok(room_id) => room_id,
                Err(_) => client.get_id_by_room_name(room_name).await?,
            };

            let info = client.live_info(room_id).await?;
            if !info.is_living() {
                return Ok(InspectResult::None);
            }

            let streams = client.live_streaming_url(room_id).await?;
            let stream = streams.best(false);

            Ok(InspectResult::Playlist(InspectPlaylist {
                title: Some(info.room_name),
                playlist_url: stream.url.clone(),
                playlist_type: PlaylistType::HLS,
                ..Default::default()
            }))
        }
    }
}
