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
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn arguments(&self, command: &mut dyn InspectorCommand) {
        command.add_argument(
            "showroom-user-session",
            Some("sr_id"),
            "[Showroom] Your Showroom user session key.",
        );
    }

    fn build(&self, args: &dyn InspectorArguments) -> anyhow::Result<Box<dyn Inspect>> {
        Ok(Box::new(ShowroomInspectorImpl(args.get_string("sr-id"))))
    }
}

struct ShowroomInspectorImpl(Option<String>);

#[async_trait]
impl Inspect for ShowroomInspectorImpl {
    async fn register(
        &self,
        id: InspectorIdentifier,
        registry: &mut InspectRegistry,
    ) -> anyhow::Result<()> {
        let client = ShowRoomClient::new(self.0.clone()).await?;

        let _client = client.clone();
        registry.register_http_route(
            RouterScheme::Https,
            "www.showroom-live.com",
            "/r/{room_name}",
            (
                id.clone(),
                Box::new(move |_, params| {
                    let client = _client.clone();
                    Box::pin(async move {
                        let room_name = params.path_params.get("room_name").unwrap();

                        let room_id = match room_name.parse::<u64>() {
                            Ok(room_id) => room_id,
                            Err(_) => client.get_id_by_room_slug(room_name).await?,
                        };

                        let info = client.live_info(room_id).await?;
                        if !info.is_living() {
                            return Ok(InspectResult::None);
                        }

                        let streams = client.live_streaming_url(room_id).await?;
                        let Some(stream) = streams.best(false) else {
                            return Ok(InspectResult::None);
                        };

                        Ok(InspectResult::Playlist(InspectPlaylist {
                            title: Some(info.room_name),
                            playlist_url: stream.url.clone(),
                            playlist_type: PlaylistType::HLS,
                            ..Default::default()
                        }))
                    })
                }),
            ),
        )?;

        registry.register_http_route(
            RouterScheme::Https,
            "www.showroom-live.com",
            "/timeshift/{room_url_key}/{view_url_key}",
            (
                id,
                Box::new(move |_, params| {
                    let client = client.clone();
                    Box::pin(async move {
                        let room_url_key = params.path_params.get("room_url_key").unwrap();
                        let view_url_key = params.path_params.get("view_url_key").unwrap();
                        let timeshift_info =
                            client.timeshift_info(room_url_key, view_url_key).await?;
                        let timeshift_streaming_url = client
                            .timeshift_streaming_url(
                                timeshift_info.timeshift.room_id,
                                timeshift_info.timeshift.live_id,
                            )
                            .await?;
                        let stream = timeshift_streaming_url.best();
                        Ok(InspectResult::Playlist(InspectPlaylist {
                            title: Some(timeshift_info.timeshift.title),
                            playlist_url: stream.url().to_string(),
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
