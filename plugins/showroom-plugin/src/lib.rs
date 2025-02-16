mod model;

use anyhow::{anyhow, Context};

use fake_user_agent::get_chrome_rua;
use model::*;
use shiori_plugin::{extism_pdk::*, *};
use url::Url;

pub struct ShowRoomClient(HttpClient);

impl ShowRoomClient {
    pub fn new() -> Self {
        ShowRoomClient(HttpClient::ua(get_chrome_rua()))
    }

    pub fn get_id_by_room_name(&self, room_name: &str) -> FnResult<u64> {
        let data: json::Value = self.0.get_json(&format!(
            "https://public-api.showroom-cdn.com/room/{room_name}"
        ))?;

        Ok(data
            .get("id")
            .ok_or_else(|| anyhow!("id not found"))?
            .as_u64()
            .ok_or_else(|| anyhow!("id is not a number"))?)
    }

    pub fn live_info(&self, room_id: u64) -> FnResult<LiveInfo> {
        let data = self
            .0
            .get_json(&format!(
                "https://www.showroom-live.com/api/live/live_info?room_id={room_id}"
            ))
            .with_context(|| "live info deserialize")?;

        Ok(data)
    }

    pub fn streaming_url(&self, room_id: u64) -> FnResult<StreamlingList> {
        let data = self
            .0
            .get_json(&format!(
                "https://www.showroom-live.com/api/live/streaming_url?room_id={room_id}&abr_available=0"
            ))
            .with_context(|| "streaming url json deserialize")?;
        Ok(data)
    }
}

#[plugin_fn]
pub fn shiori_name() -> FnResult<String> {
    Ok(String::from("showroom"))
}

#[plugin_fn]
pub fn shiori_matches(url: String) -> FnResult<Msgpack<bool>> {
    Ok(url.starts_with("https://www.showroom-live.com/r/").into())
}

#[plugin_fn]
pub fn shiori_inspect(url: String) -> FnResult<Msgpack<InspectResult>> {
    let url = Url::parse(&url)?;
    let room_name = url.path().trim_start_matches("/r/");

    let client = ShowRoomClient::new();
    let room_id = match room_name.parse::<u64>() {
        Ok(room_id) => room_id,
        Err(_) => client.get_id_by_room_name(room_name)?,
    };

    let info = client.live_info(room_id)?;
    if !info.is_living() {
        return Ok(InspectResult::None.into());
    }

    let streams = client.streaming_url(room_id)?;
    let stream = streams.best(false);

    Ok(InspectResult::Playlist(InspectPlaylist {
        title: Some(info.room_name),
        playlist_url: stream.url.clone(),
        playlist_type: PlaylistType::HLS,
        ..Default::default()
    })
    .into())
}
