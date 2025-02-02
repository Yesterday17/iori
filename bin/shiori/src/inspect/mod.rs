pub mod inspectors;
mod types;

use std::borrow::Cow;
pub use types::*;

pub async fn inspect(
    url: &str,
    inspectors: Vec<Box<dyn Inspect>>,
) -> anyhow::Result<(&'static str, InspectData)> {
    let mut url = Cow::Borrowed(url);

    for inspector in inspectors {
        if inspector.matches(&url).await {
            match inspector.inspect(&url).await? {
                InspectResult::NotMatch => continue,
                InspectResult::Playlist(data) => return Ok((inspector.name(), data)),
                InspectResult::Redirect(redirect_url) => {
                    url = Cow::Owned(redirect_url);
                }
                InspectResult::None => anyhow::bail!("Not found"),
            }
        }
    }

    anyhow::bail!("No inspector matched")
}
