use iori::{
    cache::file::FileCacheSource, dash::live2::CommonDashLiveSource,
    download::ParallelDownloaderBuilder, merge::SkipMerger, HttpClient,
};
use tracing::level_filters::LevelFilter;

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

    let mpd_url = "https://livesim.dashif.org/livesim2/segtimelinenr_1/WAVE/vectors/cfhd_sets/14.985_29.97_59.94/t1/2022-10-17/stream.mpd"; //"https://livesim.dashif.org/livesim2/testpic_2s/Manifest.mpd";

    let key_str = None;

    let client = HttpClient::default();

    let source = CommonDashLiveSource::new(client.clone(), mpd_url.parse()?, key_str)?;

    let cache_dir = std::env::temp_dir().join("iori_live_dash_example");
    tracing::info!("Using cache directory: {}", cache_dir.display());

    let cache = FileCacheSource::new(cache_dir)?;
    let merger = SkipMerger::new();

    let downloader = ParallelDownloaderBuilder::new().cache(cache).merger(merger);

    tracing::info!("Starting download for live stream: {}", mpd_url);
    match downloader.download(source).await {
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
