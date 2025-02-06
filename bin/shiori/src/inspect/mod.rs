pub mod inspectors;
mod types;

use std::borrow::Cow;
pub use types::*;

pub async fn inspect(
    url: &str,
    inspectors: Vec<Box<dyn Inspect>>,
    choose_candidate: fn(Vec<InspectCandidate>) -> InspectCandidate,
) -> anyhow::Result<(&'static str, Vec<InspectPlaylist>)> {
    let mut url = Cow::Borrowed(url);

    for inspector in inspectors {
        if inspector.matches(&url).await {
            let result = inspector.inspect(&url).await?;
            let result = handle_inspect_result(&inspector, result, choose_candidate).await?;
            match result {
                InspectBranch::Continue => (),
                InspectBranch::Redirect(redirect_url) => url = Cow::Owned(redirect_url),
                InspectBranch::Found(data) => return Ok((inspector.name(), data)),
            }
        }
    }

    anyhow::bail!("No inspector matched")
}

enum InspectBranch {
    Continue,
    Redirect(String),
    Found(Vec<InspectPlaylist>),
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
        InspectResult::None => anyhow::bail!("Not found"),
    })
}
