use std::{path::PathBuf, sync::Arc, time::Duration};

use iori::{
    dash::live::{LiveDashSource, RepresentationSelector},
    decrypt::IoriKey,
    download::{Downloader, SequencialDownloader},
    error::IoriError,
    cache::{CacheSource, FileCacheSource},
    merge::{Merger, SkipMerger},
    HttpClient,
};
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: cargo run --example live_dash -- <MPD_URL> [KEY]");
        eprintln!("\n  <MPD_URL>: URL of the live DASH MPD manifest.");
        eprintln!("  [KEY]: Optional decryption key in KEY_ID:KEY format or just KEY (if KID is not needed or embedded).");
        eprintln!("\nExample live streams to try:");
        eprintln!("  - https://livesim.dashif.org/livesim/testpic_2s/Manifest.mpd (No Key)");
        eprintln!("  - Other public live DASH streams (some might require keys).");
        anyhow::bail!("Missing MPD_URL argument");
    }

    let mpd_url_str = &args[1];
    let mpd_url = Url::parse(mpd_url_str)?;

    let key_str = args.get(2);
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
    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir)?;
    }
    tracing::info!("Using cache directory: {}", cache_dir.display());
    
    let cache = FileCacheSource::new(cache_dir)?;
    let merger: Arc<dyn Merger<DashSegment = iori::dash::segment::DashSegment>> = Arc::new(SkipMerger::new());


    let downloader = SequencialDownloader::new(live_source, merger, Arc::new(cache));

    tracing::info!("Starting download for live stream: {}", mpd_url);
    match downloader.download().await {
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
