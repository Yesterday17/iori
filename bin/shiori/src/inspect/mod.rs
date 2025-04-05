pub mod inspectors;

pub use shiori_plugin::*;
use std::{borrow::Cow, time::Duration};
use tokio::time::sleep;

use crate::commands::STYLES;

pub struct Inspectors {
    /// Whether to wait on found
    wait: bool,

    front: Vec<Box<dyn InspectorBuilder + Send + Sync + 'static>>,
    tail: Vec<Box<dyn InspectorBuilder + Send + Sync + 'static>>,
}

impl Inspectors {
    pub fn new() -> Self {
        Self {
            wait: false,
            front: Vec::new(),
            tail: Vec::new(),
        }
    }

    /// Add inspector to front queue
    pub fn add(&mut self, builder: impl InspectorBuilder + Send + Sync + 'static) -> &mut Self {
        self.front.push(Box::new(builder));
        self
    }

    pub fn push(&mut self, builder: impl InspectorBuilder + Send + Sync + 'static) -> &mut Self {
        self.tail.push(Box::new(builder));
        self
    }

    pub fn wait(mut self, value: bool) -> Self {
        self.wait = value;
        self
    }

    pub fn help(self) -> String {
        let mut is_first = true;

        let mut result = format!("{style}Inspectors:{style:#}\n", style = STYLES.get_header());

        let inspectors = self.front.iter().chain(self.tail.iter());
        for inspector in inspectors {
            if !is_first {
                result.push_str("\n");
            }
            is_first = false;

            result.push_str(&format!(
                "  {style}{}:{style:#}\n",
                inspector.name(),
                style = STYLES.get_literal()
            ));
            for line in inspector.help() {
                result.push_str(&" ".repeat(10));
                result.push_str(&line);
                result.push_str("\n");
            }
        }
        result
    }

    pub async fn inspect(
        self,
        url: &str,
        args: InspectorArgs,
        choose_candidate: fn(Vec<InspectCandidate>) -> InspectCandidate,
    ) -> anyhow::Result<(String, Vec<InspectPlaylist>)> {
        let inspectors = self
            .front
            .iter()
            .chain(self.tail.iter())
            .map(|b| b.build(&args).map(|i| (b, i)))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let mut url = Cow::Borrowed(url);

        for (builder, inspector) in inspectors {
            if inspector.matches(&url).await {
                loop {
                    let result = inspector.inspect(&url).await?;
                    let result =
                        handle_inspect_result(&inspector, result, choose_candidate).await?;
                    match result {
                        InspectBranch::Continue => break,
                        InspectBranch::Redirect(redirect_url) => {
                            url = Cow::Owned(redirect_url);
                            break;
                        }
                        InspectBranch::Found(data) => return Ok((builder.name(), data)),
                        InspectBranch::NotFound => {
                            if self.wait {
                                sleep(Duration::from_secs(5)).await;
                            } else {
                                anyhow::bail!("Not found")
                            }
                        }
                    }
                }
            }
        }

        anyhow::bail!("No inspector matched")
    }
}

enum InspectBranch {
    Continue,
    Redirect(String),
    Found(Vec<InspectPlaylist>),
    NotFound,
}

#[async_recursion::async_recursion]
async fn handle_inspect_result(
    inspector: &Box<dyn Inspect>,
    result: InspectResult,
    choose_candidate: fn(Vec<InspectCandidate>) -> InspectCandidate,
) -> anyhow::Result<InspectBranch> {
    Ok(match result {
        InspectResult::NotMatch => InspectBranch::Continue,
        InspectResult::Candidates(candidates) => {
            let candidate = choose_candidate(candidates);
            let result = inspector.inspect_candidate(candidate).await?;
            handle_inspect_result(inspector, result, choose_candidate).await?
        }
        InspectResult::Playlist(data) => InspectBranch::Found(vec![data]),
        InspectResult::Redirect(redirect_url) => InspectBranch::Redirect(redirect_url),
        InspectResult::None => InspectBranch::NotFound,
    })
}
