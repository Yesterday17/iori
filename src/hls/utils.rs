use m3u8_rs::{MediaPlaylist, Playlist};
use reqwest::{Client, Url};

#[async_recursion::async_recursion]
pub(crate) async fn load_m3u8(client: &Client, url: Url) -> (Url, MediaPlaylist) {
    log::info!("Start fetching M3U8 file.");

    let m3u8_bytes = client
        .get(url.clone())
        .send()
        .await
        .expect("http error")
        .bytes()
        .await
        .expect("Failed to get body bytes");
    log::info!("M3U8 file fetched.");

    let parsed = m3u8_rs::parse_playlist_res(m3u8_bytes.as_ref());
    match parsed {
        Ok(Playlist::MasterPlaylist(pl)) => {
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
        Ok(Playlist::MediaPlaylist(pl)) => (url, pl),
        Err(e) => panic!("Error: {:?}", e),
    }
}
