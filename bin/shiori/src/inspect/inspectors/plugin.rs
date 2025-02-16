use std::sync::{Arc, Mutex};

use crate::inspect::{Inspect, InspectResult};
use clap_handler::async_trait;
use extism::{convert::Msgpack, *};
use serde::de::DeserializeOwned;

pub struct ExtismInspector {
    name: String,
    plugin: Arc<Mutex<Plugin>>,
}

impl ExtismInspector {
    pub fn new(wasm: Wasm) -> Self {
        let name = wasm
            .meta()
            .name
            .as_deref()
            .unwrap_or_else(|| "plugin")
            .to_string();
        let manifest = Manifest::new([wasm]).with_allowed_host("*");
        let plugin = Plugin::new(&manifest, [], true).unwrap();

        Self {
            name,
            plugin: Arc::new(Mutex::new(plugin)),
        }
    }

    pub fn url(wasm_url: String) -> Self {
        let wasm = Wasm::url(wasm_url);
        Self::new(wasm)
    }

    pub fn file(path: String) -> Self {
        let wasm = Wasm::file(path);
        Self::new(wasm)
    }

    pub async fn call<Output: Send + DeserializeOwned + 'static>(
        &self,
        method: &'static str,
        input: impl ToBytes<'_> + Send + 'static,
    ) -> anyhow::Result<Output> {
        let plugin = self.plugin.clone();
        let result = tokio::task::spawn_blocking(move || {
            plugin
                .lock()
                .unwrap()
                .call::<_, Msgpack<Output>>(method, input)
                .unwrap()
                .0
        })
        .await?;
        Ok(result)
    }
}

#[async_trait]
impl Inspect for ExtismInspector {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn matches(&self, url: &str) -> bool {
        self.call("shiori_matches", url.to_string()).await.unwrap()
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        self.call("shiori_inspect", url.to_string()).await
    }
}
