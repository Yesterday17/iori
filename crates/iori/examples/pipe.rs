use std::{
    num::NonZeroU32,
    time::{SystemTime, UNIX_EPOCH},
};

use iori::{
    cache::file::FileCacheSource, download::ParallelDownloader, hls::HlsLiveSource,
    merge::PipeMerger,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let url = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: {} <m3u8_url>", std::env::args().next().unwrap());
        std::process::exit(1);
    });
    let key = std::env::args().nth(2);

    let started_at = SystemTime::now();
    let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
    let output_dir = std::env::temp_dir().join(format!("iori_pipe_{}", started_at));

    let source = HlsLiveSource::new(Default::default(), url, key.as_deref(), None);
    let merger = PipeMerger::stdout(true);
    let cache = FileCacheSource::new(output_dir)?;

    ParallelDownloader::builder()
        .cache(cache)
        .merger(merger)
        .concurrency(NonZeroU32::new(8).unwrap())
        .retries(8)
        .download(source)
        .await?;

    Ok(())
}
