use crate::inspect::{Inspect, InspectResult};
use clap_handler::async_trait;

pub struct TemplateInspector;

#[async_trait]
impl Inspect for TemplateInspector {
    fn name(&self) -> &'static str {
        "template"
    }

    async fn matches(&self, _url: &str) -> bool {
        true
    }

    async fn inspect(&self, _url: &str) -> anyhow::Result<InspectResult> {
        Ok(InspectResult::None)
    }
}
