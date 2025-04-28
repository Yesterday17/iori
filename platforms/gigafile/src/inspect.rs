use fake_user_agent::get_chrome_rua;
use regex::bytes::Regex;
use reqwest::{
    header::{CONTENT_DISPOSITION, COOKIE, USER_AGENT},
    Client,
};
use shiori_plugin::{
    async_trait, Inspect, InspectPlaylist, InspectResult, InspectorBuilder, PlaylistType,
};

use crate::client::GigafileClient;

pub struct GigafileInspector;

impl InspectorBuilder for GigafileInspector {
    fn name(&self) -> String {
        "gigafile".to_string()
    }

    fn help(&self) -> Vec<String> {
        [
            "Extracts raw download URL from Gigafile.",
            "",
            "Template:",
            "- https://*.gigafile.nu/*",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn arguments(&self, command: &mut dyn shiori_plugin::InspectorCommand) {
        command.add_argument("gigafile-key", Some("key"), "[Gigafile] Download key");
    }

    fn build(
        &self,
        args: &dyn shiori_plugin::InspectorArguments,
    ) -> anyhow::Result<Box<dyn shiori_plugin::Inspect>> {
        Ok(Box::new(GigafileInspectorImpl(
            args.get_string("gigafile-key"),
        )))
    }
}

struct GigafileInspectorImpl(Option<String>);

#[async_trait]
impl Inspect for GigafileInspectorImpl {
    async fn matches(&self, url: &str) -> bool {
        let re = Regex::new(r"^https://\d+\.gigafile\.nu/.*").unwrap();
        re.is_match(url.as_bytes())
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        let client = GigafileClient::new(self.0.clone());
        let (url, cookie) = client.get_download_url(url).await?;

        let client = Client::new();
        let response = client
            .get(&url)
            .header(COOKIE, &cookie)
            .header(USER_AGENT, get_chrome_rua())
            .send()
            .await?;
        let filename = response.headers().get(CONTENT_DISPOSITION).and_then(|v| {
            // attachment; filename="<filename>";
            let re = Regex::new(r#"filename="([^"]+)"#).unwrap();
            let matched = re
                .captures(v.as_bytes())
                .and_then(|c| c.get(1).map(|m| m.as_bytes()))?;
            let filename = String::from_utf8(matched.to_vec()).ok()?;
            Some(filename)
        });
        drop(response);

        let filename = filename.and_then(|f| {
            let (name, ext) = f.rsplit_once('.').unwrap_or((&f, "raw"));
            Some((name.to_string(), ext.to_string()))
        });
        let (title, ext) = match filename {
            Some((filename, ext)) => (Some(filename), ext),
            None => (None, "raw".to_string()),
        };

        Ok(InspectResult::Playlist(InspectPlaylist {
            title: title,
            playlist_url: url,
            playlist_type: PlaylistType::Raw(ext),
            headers: vec![format!("Cookie: {cookie}")],
            ..Default::default()
        }))
    }
}
