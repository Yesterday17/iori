use crate::inspect::{Inspect, InspectCandidate, InspectResult};
use base64::{prelude::BASE64_STANDARD, Engine};
use clap_handler::async_trait;
use std::process::{Command, Stdio};

pub struct ExternalInspector {
    program: String,
    args: Vec<String>,
}

impl ExternalInspector {
    pub fn new(command: &str) -> anyhow::Result<Self> {
        let result = shlex::split(command).unwrap_or_default();
        let program = result
            .first()
            .ok_or_else(|| anyhow::anyhow!("Invalid command"))?
            .to_string();
        let args = result.into_iter().skip(1).map(|s| s.to_string()).collect();

        Ok(ExternalInspector { program, args })
    }
}

#[async_trait]
impl Inspect for ExternalInspector {
    fn name(&self) -> &'static str {
        "external"
    }

    async fn matches(&self, _url: &str) -> bool {
        true
    }

    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult> {
        let mut child = Command::new(&self.program)
            .args(self.args.as_slice())
            .arg("inspect")
            .arg(url)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let Some(stdout) = child.stdout.take() else {
            return Err(anyhow::anyhow!("Failed to get external output"));
        };
        let data: InspectResult = rmp_serde::from_read(stdout)?;
        Ok(data)
    }

    async fn inspect_candidate(
        &self,
        candidate: InspectCandidate,
    ) -> anyhow::Result<InspectResult> {
        let mut child = Command::new(&self.program)
            .args(self.args.as_slice())
            .arg("inspect-candidate")
            .arg({
                let candidate = rmp_serde::to_vec(&candidate)?;
                BASE64_STANDARD.encode(candidate)
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let Some(stdout) = child.stdout.take() else {
            return Err(anyhow::anyhow!("Failed to get stdout"));
        };
        let data: InspectResult = rmp_serde::from_read(stdout)?;
        Ok(data)
    }
}
