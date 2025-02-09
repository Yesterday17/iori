pub mod inspectors;
mod types;

use std::{borrow::Cow, time::Duration};
use tokio::time::sleep;
pub use types::*;

pub async fn inspect(
    url: &str,
    inspectors: Vec<Box<dyn Inspect>>,
    choose_candidate: fn(Vec<InspectCandidate>) -> InspectCandidate,
    wait_on_not_found: bool,
) -> anyhow::Result<(&'static str, Vec<InspectPlaylist>)> {
    let mut url = Cow::Borrowed(url);

    for inspector in inspectors {
        if inspector.matches(&url).await {
            loop {
                let result = inspector.inspect(&url).await?;
                let result = handle_inspect_result(&inspector, result, choose_candidate).await?;
                match result {
                    InspectBranch::Continue => break,
                    InspectBranch::Redirect(redirect_url) => {
                        url = Cow::Owned(redirect_url);
                        break;
                    }
                    InspectBranch::Found(data) => return Ok((inspector.name(), data)),
                    InspectBranch::NotFound => {
                        if wait_on_not_found {
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
