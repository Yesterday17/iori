use std::sync::Arc;

use iori::{
    cache::file::FileCacheSource,
    dash::live::{LiveDashSource, RepresentationSelector},
    decrypt::IoriKey,
    download::ParallelDownloaderBuilder,
    merge::SkipMerger,
    HttpClient, IoriError,
};
use tracing::level_filters::LevelFilter;
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .try_from_env()
                .unwrap_or_else(|_| "info,i18n_embed::requester=off".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let mpd_url_str = "https://livesim.dashif.org/livesim2/testpic_2s/Manifest.mpd";
    let mpd_url = Url::parse(mpd_url_str)?;

    let key_str = None;
    let iori_key = if let Some(k) = key_str {
        // Assuming IoriKey::clear_key can handle "KEY_ID:KEY" or just "KEY"
        // This might need adjustment based on IoriKey's actual parsing logic
        Some(Arc::new(IoriKey::clear_key(k)?))
    } else {
        None
    };

    let client = HttpClient::default();

    // Default representation selector: selects the representation with the maximum bandwidth.
    let representation_selector: RepresentationSelector = Arc::new(|representations| {
        representations
            .iter()
            .max_by_key(|r| r.bandwidth.unwrap_or(0))
            .cloned()
            .ok_or_else(|| IoriError::NoRepresentationFound)
    });

    let live_source = LiveDashSource::new(
        client.clone(),
        mpd_url.clone(),
        iori_key,
        None, // shaka_packager_command (optional)
        Some(representation_selector),
    );

    let cache_dir = std::env::temp_dir().join("iori_live_dash_example");
    tracing::info!("Using cache directory: {}", cache_dir.display());

    let cache = FileCacheSource::new(cache_dir)?;
    let merger = SkipMerger::new();

    let downloader = ParallelDownloaderBuilder::new().cache(cache).merger(merger);

    tracing::info!("Starting download for live stream: {}", mpd_url);
    match downloader.download(live_source).await {
        Ok(_) => {
            tracing::info!("Live stream download finished or stopped (e.g., MPD became static or updater task ended).");
        }
        Err(e) => {
            tracing::error!("Download error: {:?}", e);
            anyhow::bail!("Download failed: {}", e);
        }
    }

    Ok(())
}
