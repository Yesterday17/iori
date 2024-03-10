use m3u8_rs::{MediaPlaylist, Playlist};
use reqwest::{Client, Url};

use crate::error::{IoriError, IoriResult};

#[async_recursion::async_recursion]
pub async fn load_m3u8(client: &Client, url: Url) -> IoriResult<(Url, MediaPlaylist)> {
    log::info!("Start fetching M3U8 file.");

    let mut retry = 3;
    let m3u8_bytes = loop {
        if retry == 0 {
            return Err(IoriError::M3u8FetchError);
        }

        match client.get(url.clone()).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => break bytes,
                Err(error) => {
                    log::warn!("Failed to fetch M3U8 file: {error}");
                    retry -= 1;
                }
            },
            Err(error) => {
                log::warn!("Failed to fetch M3U8 file: {error}");
                retry -= 1;
            }
        }
    };
    log::info!("M3U8 file fetched.");

    let parsed = m3u8_rs::parse_playlist_res(&m3u8_bytes)
        .map_err(|_| IoriError::M3u8ParseError(String::from_utf8_lossy(&m3u8_bytes).to_string()))?;
    match parsed {
        Playlist::MasterPlaylist(pl) => {
            log::info!("Master playlist input detected. Auto selecting best quality streams.");
            let mut variants = pl.variants;
            variants.sort_by(|a, b| {
                // compare resolution first
                if let (Some(a), Some(b)) = (a.resolution, b.resolution) {
                    if a.width != b.width {
                        return b.width.cmp(&a.width);
                    }
                }

                // compare framerate then
                if let (Some(a), Some(b)) = (a.frame_rate, b.frame_rate) {
                    let a = a as u64;
                    let b = b as u64;
                    if a != b {
                        return b.cmp(&a);
                    }
                }

                // compare bandwidth finally
                b.bandwidth.cmp(&a.bandwidth)
            });
            let variant = variants.get(0).expect("No variant found");
            let url = url.join(&variant.uri).expect("Invalid variant uri");

            log::info!(
                "Best stream: {url}; Bandwidth: {bandwidth}",
                bandwidth = variant.bandwidth
            );
            load_m3u8(client, url).await
        }
        Playlist::MediaPlaylist(pl) => Ok((url, pl)),
    }
}
