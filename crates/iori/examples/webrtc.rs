use std::time::{SystemTime, UNIX_EPOCH};

use iori::{
    cache::file::FileCacheSource, download::SequencialDownloader, merge::SkipMerger,
    webrtc::WebRTCLiveSource,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let started_at = SystemTime::now();
    let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
    let output_dir = std::env::temp_dir().join(format!("iori_save_{}", started_at));
    log::info!("Caching to {}", output_dir.display());

    let source = WebRTCLiveSource {};
    let merger = SkipMerger::new();
    let cache = FileCacheSource::new(output_dir)?;

    let mut downloader = SequencialDownloader::new(source, merger, cache);
    downloader.download().await?;

    Ok(())
}
