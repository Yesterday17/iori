use std::sync::LazyLock;

use regex::Regex;
use reqwest::Client;

use crate::source::NicoTimeshiftSource;

const NICO_METADATA_REGEXP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<script id="embedded-data" data-props="([^"]+)""#).unwrap());

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
        self.data
            .get("site")
            .and_then(|site| site.get("relive"))
            .and_then(|relive| relive.get("webSocketUrl"))
            .and_then(|url| url.as_str())
            .and_then(|url| if url.is_empty() { None } else { Some(url) })
            .map(|url| url.to_string())
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

    pub async fn get_source(
        &self,
        // title: Option<String>,
    ) -> anyhow::Result<NicoTimeshiftSource> {
        // let title = title.unwrap_or_else(|| self.program_title());
        let wss_url = self
            .websocket_url()
            .ok_or_else(|| anyhow::anyhow!("no websocket url"))?;

        let source = NicoTimeshiftSource::new(Default::default(), wss_url).await?;
        Ok(source)
    }
}

#[cfg(test)]
mod tests {
    use super::NicoEmbeddedData;

    #[tokio::test]
    async fn test_get_live_info() -> anyhow::Result<()> {
        let data =
            NicoEmbeddedData::new("https://live.nicovideo.jp/watch/lv342260645", None).await?;
        println!("{:?}", data.websocket_url());
        Ok(())
    }
}
