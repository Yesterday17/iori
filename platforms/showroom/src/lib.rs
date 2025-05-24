pub mod constants;
pub mod inspect;
pub mod model;

use anyhow::{anyhow, Context};
use fake_user_agent::get_chrome_rua;
use model::*;
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE, SET_COOKIE},
    Client,
};

#[derive(Clone)]
pub struct ShowRoomClient {
    client: Client,
    sr_id: String,
}

impl PartialEq for ShowRoomClient {
    fn eq(&self, other: &Self) -> bool {
        self.sr_id.eq(&other.sr_id)
    }
}

async fn get_sr_id() -> anyhow::Result<String> {
    let response = reqwest::get("https://www.showroom-live.com/api/live/onlive_num").await?;
    let cookies = response.headers().get_all(SET_COOKIE);
    for cookie in cookies {
        let Some((kv, _)) = cookie.to_str()?.split_once(';') else {
            continue;
        };
        let Some((key, value)) = kv.split_once('=') else {
            continue;
        };
        if key == "sr_id" {
            return Ok(value.to_string());
        }
    }

    // fallback guest id
    Ok("u9ZYLQddhas3AEWr7t2ohQ-zHaVWmkuVg9IGr5IWtTr6-S2U24EA3e4jgg1nSL0Q".to_string())
}

impl ShowRoomClient {
    /// Key saved from: https://hls-archive-aes.live.showroom-live.com/aes.key
    /// Might be useful for timeshift
    pub const ARCHIVE_KEY: &str = "2a63847146f96dd3a17077f6c72daffb";

    pub async fn new(sr_id: Option<String>) -> anyhow::Result<Self> {
        let mut builder = Client::builder()
            .user_agent(get_chrome_rua())
            .connection_verbose(true);

        let sr_id = match sr_id {
            Some(s) => s,
            None => get_sr_id().await?,
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_str(&format!(
                "sr_id={sr_id}; uuid=b950e897-c6ab-46bc-828f-fa231a73cf3d; i18n_redirected=ja"
            ))
            .expect("sr_id is not a valid header value"),
        );
        builder = builder.default_headers(headers);

        Ok(Self {
            client: builder.build().unwrap(),
            sr_id,
        })
    }

    pub async fn get_id_by_room_slug(&self, room_slug: &str) -> anyhow::Result<u64> {
        let data: serde_json::Value = self
            .client
            .get(format!(
                "https://public-api.showroom-cdn.com/room/{room_slug}"
            ))
            .send()
            .await?
            .json()
            .await?;

        data
            .get("id")
            .ok_or_else(|| anyhow!("id not found"))?
            .as_u64()
            .ok_or_else(|| anyhow!("id is not a number"))
    }

    pub async fn room_profile(&self, room_id: u64) -> anyhow::Result<RoomProfile> {
        let data = self
            .client
            .get("https://www.showroom-live.com/api/room/profile")
            .query(&[("room_id", room_id)])
            .send()
            .await?
            .json()
            .await
            .with_context(|| "room profile deserialize")?;

        Ok(data)
    }

    pub async fn live_info(&self, room_id: u64) -> anyhow::Result<LiveInfo> {
        let data = self
            .client
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
            .client
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
            .client
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
            .client
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
        let client = ShowRoomClient::new(None).await.unwrap();
        let room_id = client.get_id_by_room_slug(S46_NAGISA_KOJIMA).await.unwrap();
        assert_eq!(room_id, 479510);
    }
}
