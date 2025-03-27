use clap::Parser;
use clap_handler::handler;
use self_update::{cargo_crate_version, get_target};

#[derive(Parser, Clone, Default, Debug)]
#[clap(name = "update")]
/// Update the shiori binary
pub struct UpdateCommand {
    /// Custom URL for the versions file
    #[clap(
        long,
        default_value = "https://raw.githubusercontent.com/Yesterday17/iori/refs/heads/master/.versions/shiori"
    )]
    versions_url: String,

    /// Target version to update to
    #[clap(short = 'v', long)]
    version: Option<String>,

    /// Target platform
    #[clap(long)]
    target: Option<String>,

    #[clap(short = 'y', long = "yes")]
    skip_confirm: bool,
}

#[handler(UpdateCommand)]
pub async fn update_command(me: UpdateCommand) -> anyhow::Result<()> {
    let target = me.target.unwrap_or_else(|| get_target().to_string());
    let target_version_tag = if let Some(version) = me.version {
        format!("shiori-v{version}")
    } else {
        reqwest::get(me.versions_url).await?.text().await?
    };

    let status = self_update::backends::github::Update::configure()
        .repo_owner("Yesterday17")
        .repo_name("iori")
        .bin_name("shiori")
        .target(&target)
        .target_version_tag(&target_version_tag)
        .show_download_progress(true)
        .current_version(cargo_crate_version!())
        .no_confirm(me.skip_confirm)
        .build()?
        .update()?;

    println!("Update status: `{}`!", status.updated());

    Ok(())
}

pub(crate) async fn check_update() -> anyhow::Result<()> {
    let current_version = format!("shiori-v{}", cargo_crate_version!());

    let latest = reqwest::Client::new()
        .get(
            "https://raw.githubusercontent.com/Yesterday17/iori/refs/heads/master/.versions/shiori",
        )
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .text()
        .await?;

    if current_version == latest {
        return Ok(());
    }
    log::info!(
        "Update available: {}. Please run `shiori update` to update.",
        latest
    );

    Ok(())
}
