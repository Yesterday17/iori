use std::time::{SystemTime, UNIX_EPOCH};

use iori::{
    consumer::Consumer, dash::archive::CommonDashArchiveSource, download::SequencialDownloader,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let url = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: {} <mpd_url>", std::env::args().next().unwrap());
        std::process::exit(1);
    });
    let key = std::env::args().nth(2);

    let started_at = SystemTime::now();
    let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
    let output_dir = std::env::temp_dir().join(format!("iori_save_{}", started_at));

    let source =
        CommonDashArchiveSource::new(Default::default(), url, key, Consumer::file(output_dir)?)?;

    let mut downloader = SequencialDownloader::new(source);
    downloader.download().await?;

    Ok(())
}
