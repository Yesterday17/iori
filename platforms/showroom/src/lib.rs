pub mod constants;
pub mod inspect;
pub mod model;

use anyhow::{anyhow, Context};
use fake_user_agent::get_chrome_rua;
use model::*;
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE},
    Client,
};

#[derive(Clone)]
pub struct ShowRoomClient(Client);

impl ShowRoomClient {
    /// Key saved from: https://hls-archive-aes.live.showroom-live.com/aes.key
    /// Might be useful for timeshift
    pub const ARCHIVE_KEY: &str = "2a63847146f96dd3a17077f6c72daffb";

    pub fn new(sr_id: Option<String>) -> Self {
        let mut builder = Client::builder()
            .user_agent(get_chrome_rua())
            .connection_verbose(true);

        if let Some(sr_id) = sr_id {
            let mut headers = HeaderMap::new();
            headers.insert(
                COOKIE,
                HeaderValue::from_str(&format!("sr_id={sr_id}"))
                    .expect("sr_id is not a valid header value"),
            );

            builder = builder.default_headers(headers);
        }

        Self(builder.build().unwrap())
    }

    pub async fn get_id_by_room_name(&self, room_name: &str) -> anyhow::Result<u64> {
        let data: serde_json::Value = self
            .0
            .get(&format!(
                "https://public-api.showroom-cdn.com/room/{room_name}"
            ))
            .send()
            .await?
            .json()
            .await?;

        Ok(data
            .get("id")
            .ok_or_else(|| anyhow!("id not found"))?
            .as_u64()
            .ok_or_else(|| anyhow!("id is not a number"))?)
    }

    pub async fn live_info(&self, room_id: u64) -> anyhow::Result<LiveInfo> {
        let data = self
            .0
            .get("https://www.showroom-live.com/api/live/live_info")
            .query(&[("room_id", room_id)])
            .send()
            .await?
            .json()
            .await
            .with_context(|| "live info deserialize")?;

        Ok(data)
    }

    pub async fn live_streaming_url(&self, room_id: u64) -> anyhow::Result<LiveStreamlingList> {
        let data = self
            .0
            .get("https://www.showroom-live.com/api/live/streaming_url")
            .query(&[
                ("room_id", room_id.to_string()),
                ("abr_available", "0".to_string()),
            ])
            .send()
            .await?
            .json()
            .await
            .with_context(|| "streaming url json deserialize")?;
        Ok(data)
    }

    pub async fn timeshift_info(
        &self,
        room_url_key: &str,
        view_url_key: &str,
    ) -> anyhow::Result<TimeshiftInfo> {
        // https://www.showroom-live.com/api/timeshift/find?room_url_key=stu48_8th_Empathy_&view_url_key=K86763
        let data = self
            .0
            .get("https://www.showroom-live.com/api/timeshift/find")
            .query(&[
                ("room_url_key", room_url_key),
                ("view_url_key", view_url_key),
            ])
            .send()
            .await?
            .json()
            .await
            .with_context(|| "timeshift info json deserialize")?;
        Ok(data)
    }

    pub async fn timeshift_streaming_url(
        &self,
        room_id: u64,
        live_id: u64,
    ) -> anyhow::Result<TimeshiftStreamingList> {
        let data = self
            .0
            .get("https://www.showroom-live.com/api/timeshift/streaming_url")
            .query(&[("room_id", room_id), ("live_id", live_id)])
            .send()
            .await?
            .json()
            .await
            .with_context(|| "timeshift streaming url json deserialize")?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use crate::{constants::S46_NAGISA_KOJIMA, ShowRoomClient};

    #[tokio::test]
    async fn test_get_id_by_room_name() {
        let client = ShowRoomClient::new(None);
        let room_id = client.get_id_by_room_name(S46_NAGISA_KOJIMA).await.unwrap();
        assert_eq!(room_id, 479510);
    }
}
