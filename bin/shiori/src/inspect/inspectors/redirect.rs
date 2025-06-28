use crate::inspect::{Inspect, InspectResult};
use clap_handler::async_trait;
use reqwest::redirect::Policy;
use shiori_plugin::*;

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

#[async_trait]
impl Inspect for ShortLinkInspector {
    async fn register(
        &self,
        id: InspectorIdentifier,
        registry: &mut InspectRegistry,
    ) -> anyhow::Result<()> {
        registry.register_http_route(
            RouterScheme::Https,
            "t.co",
            "/{id}",
            (
                id,
                Box::new(move |url, _| {
                    Box::pin(async move {
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
                    })
                }),
            ),
        )?;

        Ok(())
    }
}
