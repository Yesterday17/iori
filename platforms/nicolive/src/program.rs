use std::sync::LazyLock;

use fake_user_agent::get_chrome_rua;
use regex::Regex;
use reqwest::{header::SET_COOKIE, Client};

static NICO_METADATA_REGEXP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<script id="embedded-data" data-props="([^"]+)""#).unwrap());

static NICO_SERVER_RESPONSE_REGEXP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<meta name="server-response" content="([^"]+)"#).unwrap());

#[derive(Debug)]
pub struct NicoEmbeddedData {
    client: Client,
    data: serde_json::Value,
}

impl NicoEmbeddedData {
    pub async fn new<S>(live_url: S, user_session: Option<&str>) -> anyhow::Result<Self>
    where
        S: AsRef<str>,
    {
        let mut headers = reqwest::header::HeaderMap::new();
        let user_session = user_session.unwrap_or_default();
        headers.insert(
            reqwest::header::COOKIE,
            reqwest::header::HeaderValue::from_str(&format!("user_session={user_session}"))?,
        );
        let client = Client::builder().default_headers(headers).build()?;

        let live_url = if live_url.as_ref().starts_with("lv") {
            &format!("https://live.nicovideo.jp/watch/{}", live_url.as_ref())
        } else {
            live_url.as_ref()
        };

        let response = client.get(live_url).send().await?;
        let text = response.text().await?;
        let json = NICO_METADATA_REGEXP
            .captures(&text)
            .and_then(|cap| cap.get(1))
            .map(|capture| {
                let capture = capture.as_str();
                // url decode
                html_escape::decode_html_entities(capture).to_string()
            })
            .unwrap();

        Ok(Self {
            client,
            data: serde_json::from_str(&json)?,
        })
    }

    pub async fn timeshift_reserve(&self) -> anyhow::Result<()> {
        let vid = self
            .data
            .get("program")
            .and_then(|program| program.get("nicoliveProgramId"))
            .and_then(|url| url.as_str())
            .unwrap();
        let url = format!("https://live2.nicovideo.jp/api/v2/programs/{vid}/timeshift/reservation");
        let _response = self.client.post(url).send().await?;

        Ok(())
    }

    pub fn websocket_url(&self) -> Option<String> {
        let url = self.raw_websocket_url()?;
        if let Some(frontend_id) = self.frontend_id() {
            let mut url = url::Url::parse(&url).ok()?;
            url.query_pairs_mut()
                .append_pair("frontend_id", &frontend_id.to_string());
            Some(url.to_string())
        } else {
            Some(url)
        }
    }

    fn raw_websocket_url(&self) -> Option<String> {
        self.data
            .get("site")
            .and_then(|site| site.get("relive"))
            .and_then(|relive| relive.get("webSocketUrl"))
            .and_then(|url| url.as_str())
            .and_then(|url| if url.is_empty() { None } else { Some(url) })
            .map(|url| url.to_string())
    }

    fn frontend_id(&self) -> Option<i64> {
        self.data
            .get("site")
            .and_then(|site| site.get("frontendId"))
            .and_then(|id| id.as_i64())
    }

    pub fn program_title(&self) -> String {
        self.data
            .get("program")
            .and_then(|program| program.get("title"))
            .and_then(|title| title.as_str())
            .map(|title| title.to_string())
            .unwrap()
    }

    pub fn program_description(&self) -> String {
        self.data
            .get("program")
            .and_then(|program| program.get("description"))
            .and_then(|description| description.as_str())
            .map(|description| description.to_string())
            .unwrap()
    }

    pub fn program_end_time(&self) -> u64 {
        self.data
            .get("program")
            .and_then(|program| program.get("endTime"))
            .and_then(|end_at| end_at.as_u64())
            .unwrap()
    }

    pub fn audience_token(&self) -> anyhow::Result<String> {
        let wss_url = self
            .websocket_url()
            .ok_or_else(|| anyhow::anyhow!("no websocket url"))?;

        let (_, audience_token) = wss_url
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("can not extract audience token from url: {wss_url}"))?;

        Ok(audience_token.to_string())
    }

    pub fn best_quality(&self) -> anyhow::Result<String> {
        let quality = self
            .data
            .get("program")
            .and_then(|program| program.get("stream"))
            .and_then(|stream| stream.as_object())
            .and_then(|stream| stream.get("maxQuality"))
            .and_then(|quality| quality.as_str())
            .ok_or_else(|| anyhow::anyhow!("no max quality"))?;

        Ok(quality.to_string())
    }
}

pub struct NivoServerResponse {
    client: Client,
    data: serde_json::Value,
}

impl NivoServerResponse {
    pub async fn new<S>(video_url: S, user_session: Option<&str>) -> anyhow::Result<Self>
    where
        S: AsRef<str>,
    {
        let mut headers = reqwest::header::HeaderMap::new();
        let user_session = user_session.unwrap_or_default();
        headers.insert(
            reqwest::header::COOKIE,
            reqwest::header::HeaderValue::from_str(&format!("user_session={user_session}"))?,
        );
        let client = Client::builder()
            .default_headers(headers)
            .user_agent(get_chrome_rua())
            .build()?;

        let live_url = if video_url.as_ref().starts_with("so") {
            &format!("https://www.nicovideo.jp/watch/{}", video_url.as_ref())
        } else {
            video_url.as_ref()
        };

        let response = client.get(live_url).send().await?;
        let text = response.text().await?;
        let json = NICO_SERVER_RESPONSE_REGEXP
            .captures(&text)
            .and_then(|cap| cap.get(1))
            .map(|capture| {
                let capture = capture.as_str();
                // url decode
                html_escape::decode_html_entities(capture).to_string()
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Failed to extract server response from the web page")
            })?;

        Ok(Self {
            client,
            data: serde_json::from_str(&json)?,
        })
    }

    pub async fn playlist_url(&self) -> anyhow::Result<(String, String)> {
        let video_id = self
            .video_id()
            .ok_or_else(|| anyhow::anyhow!("no video id"))?;
        let url = format!("https://nvapi.nicovideo.jp/v1/watch/{video_id}/access-rights/hls",);

        let action_track_id = self
            .action_track_id()
            .ok_or_else(|| anyhow::anyhow!("no action track id"))?;
        let access_right_key = self
            .access_right_key()
            .ok_or_else(|| anyhow::anyhow!("no access right key"))?;

        let video_qualities = self
            .video_qualities()
            .ok_or_else(|| anyhow::anyhow!("no video quality"))?;
        let audio_qualities = self
            .audio_qualities()
            .ok_or_else(|| anyhow::anyhow!("no audio quality"))?;
        let mut outputs = Vec::with_capacity(video_qualities.len() * audio_qualities.len());
        for video_quality in video_qualities.iter() {
            for audio_quality in audio_qualities.iter() {
                outputs.push([video_quality.as_str(), audio_quality.as_str()]);
            }
        }

        let json = serde_json::json!({
            "outputs": outputs,
        });

        let response = self
            .client
            .post(url)
            .query(&[("actionTrackId", action_track_id)])
            .header("x-access-right-key", access_right_key)
            .header("x-frontend-id", "6")
            .header("x-frontend-version", "0")
            .header("x-niconico-language", "ja-JP")
            .header("x-request-with", "nicovideo")
            .json(&json)
            .send()
            .await?;

        let cookies = response.headers().get_all(SET_COOKIE);
        let cookies = cookies
            .into_iter()
            .filter_map(|cookie| {
                let Ok(cookie) = cookie.to_str() else {
                    return None;
                };
                let (kv, _) = cookie.split_once(';')?;
                Some(kv)
            })
            .collect::<Vec<_>>()
            .join("; ");

        let data: serde_json::Value = response.json().await?;
        let url = data["data"]["contentUrl"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no url"))?;

        Ok((url.to_string(), cookies))
    }

    fn response(&self) -> Option<&serde_json::Value> {
        self.data.get("data").and_then(|data| data.get("response"))
    }

    fn domand(&self) -> Option<&serde_json::Value> {
        self.response()
            .and_then(|r| r.get("media"))
            .and_then(|m| m.get("domand"))
    }

    pub fn program_title(&self) -> Option<String> {
        self.response()
            .and_then(|r| r.get("video"))
            .and_then(|video| video.get("title"))
            .and_then(|title| title.as_str())
            .map(|title| title.to_string())
    }

    fn video_id(&self) -> Option<String> {
        self.response()
            .and_then(|r| r.get("video"))
            .and_then(|video| video.get("id"))
            .and_then(|id| id.as_str())
            .map(|id| id.to_string())
    }

    fn access_right_key(&self) -> Option<String> {
        self.domand()
            .and_then(|media| media.get("accessRightKey"))
            .and_then(|key| key.as_str())
            .map(|key| key.to_string())
    }

    fn video_qualities(&self) -> Option<Vec<String>> {
        self.domand()
            .and_then(|media| media.get("videos"))
            .and_then(|quality| quality.as_array())
            .and_then(|q| {
                q.iter()
                    .filter(|q| {
                        q.get("isAvailable")
                            .and_then(|q| q.as_bool())
                            .unwrap_or_default()
                    })
                    .map(|q| q.get("id").and_then(|q| q.as_str()).map(String::from))
                    .collect::<Option<Vec<_>>>()
            })
    }

    fn audio_qualities(&self) -> Option<Vec<String>> {
        self.domand()
            .and_then(|media| media.get("audios"))
            .and_then(|quality| quality.as_array())
            .and_then(|q| {
                q.iter()
                    .filter(|q| {
                        q.get("isAvailable")
                            .and_then(|q| q.as_bool())
                            .unwrap_or_default()
                    })
                    .map(|q| q.get("id").and_then(|q| q.as_str()).map(String::from))
                    .collect::<Option<Vec<_>>>()
            })
    }

    fn action_track_id(&self) -> Option<String> {
        self.response()
            .and_then(|r| r.get("client"))
            .and_then(|client| client.get("watchTrackId"))
            .and_then(|id| id.as_str())
            .map(|id| id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use crate::program::NivoServerResponse;

    use super::NicoEmbeddedData;

    #[tokio::test]
    async fn test_get_live_info() -> anyhow::Result<()> {
        let data =
            NicoEmbeddedData::new("https://live.nicovideo.jp/watch/lv347149115", None).await?;
        println!("{:?}", data.websocket_url());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_playlist() -> anyhow::Result<()> {
        let data =
            NivoServerResponse::new("https://www.nicovideo.jp/watch/so45023417", None).await?;
        println!("{:?}", data.playlist_url().await?);
        Ok(())
    }
}
