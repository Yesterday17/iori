pub mod constants;
pub mod inspect;
pub mod model;

use anyhow::{anyhow, Context};
use fake_user_agent::get_chrome_rua;
use model::*;
use reqwest::Client;

pub struct ShowRoomClient(Client);

impl ShowRoomClient {
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

    pub async fn streaming_url(&self, room_id: u64) -> anyhow::Result<StreamlingList> {
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
}

impl Default for ShowRoomClient {
    fn default() -> Self {
        Self(
            Client::builder()
                .user_agent(get_chrome_rua())
                .connection_verbose(true)
                .build()
                .unwrap(),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{constants::S46_NAGISA_KOJIMA, ShowRoomClient};

    #[tokio::test]
    async fn test_get_id_by_room_name() {
        let client = ShowRoomClient::default();
        let room_id = client.get_id_by_room_name(S46_NAGISA_KOJIMA).await.unwrap();
        assert_eq!(room_id, 479510);
    }
}
