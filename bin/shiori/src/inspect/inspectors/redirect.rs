use crate::inspect::{Inspect, InspectResult};
use clap_handler::async_trait;
use regex::Regex;
use reqwest::redirect::Policy;
use shiori_plugin::{InspectorArguments, InspectorBuilder};
use std::sync::LazyLock;

pub struct ShortLinkInspector;

impl InspectorBuilder for ShortLinkInspector {
    fn name(&self) -> String {
        "redirect".to_string()
    }

    fn help(&self) -> Vec<String> {
        [
            "Redirects shortlinks to the original URL.",
            "",
            "Available services:",
            "- X/Twitter: https://t.co/*",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn build(&self, _args: &dyn InspectorArguments) -> anyhow::Result<Box<dyn Inspect>> {
        Ok(Box::new(Self))
    }
}

static TWITTER_SHORT_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| regex::Regex::new(r"https://t.co/\w+").unwrap());

#[async_trait]
impl Inspect for ShortLinkInspector {
    async fn matches(&self, url: &str) -> bool {
        TWITTER_SHORT_LINK_REGEX.is_match(url)
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .redirect(Policy::none())
            .build()?;
        let response = client.head(url).send().await?;
        let location = response
            .headers()
            .get("location")
            .and_then(|l| l.to_str().ok());

        if let Some(location) = location {
            Ok(InspectResult::Redirect(location.to_string()))
        } else {
            Ok(InspectResult::None)
        }
    }
}
