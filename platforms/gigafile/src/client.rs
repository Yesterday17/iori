use anyhow::Result;
use fake_user_agent::get_chrome_rua;
use reqwest::header::SET_COOKIE;
use reqwest::Client;

pub struct GigafileClient {
    client: Client,
    key: Option<String>,
}

impl GigafileClient {
    pub fn new(key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(get_chrome_rua())
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        Self { client, key }
    }

    pub async fn get_download_url(
        &self,
        url: &str,
    ) -> Result<(String /* url */, String /* cookies */)> {
        let response = self.client.head(url).send().await?;
        let mut cookie = String::new();
        for s in response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .map(|c| c.to_str())
        {
            let s = s?;
            let (entry, _) = s.split_once(';').unwrap_or((s, ""));
            cookie += entry;
            cookie += "; ";
        }
        cookie.pop();
        cookie.pop();

        let (domain, file_id) = url.rsplit_once('/').unwrap();
        let mut download_url = format!("{domain}/download.php?file={file_id}");

        if let Some(key) = &self.key {
            download_url.push_str(&format!("&dlkey={}", key));
        }

        Ok((download_url, cookie))
    }
}
