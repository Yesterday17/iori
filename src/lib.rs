#![allow(async_fn_in_trait)]
use std::{str::FromStr, sync::Arc};

use futures_util::StreamExt;
use m3u8_rs::{MediaPlaylist, Playlist};
use reqwest::{Client, Url};
use tokio::sync::mpsc;

/// ┌───────────────────────┐                ┌────────────────────┐
/// │                       │    Segment 1   │                    │
/// │                       ├────────────────►                    ├───┐
/// │                       │                │                    │   │fetch_segment
/// │                       │    Segment 2   │                    ◄───┘
/// │      M3U8 Time#1      ├────────────────►     Downloader     │
/// │                       │                │                    ├───┐
/// │                       │    Segment 3   │       [MPSC]       │   │fetch_segment
/// │                       ├────────────────►                    ◄───┘
/// │                       │                │                    │
/// └───────────────────────┘                │                    ├───┐
///                                          │                    │   │fetch_segment
/// ┌───────────────────────┐                │                    ◄───┘
/// │                       │       ...      │                    │
/// │                       ├────────────────►                    │
/// │                       │                │                    │
/// │      M3U8 Time#N      │                │                    │
/// │                       │                │                    │
/// │                       │                │                    │
/// │                       │  Segment Last  │                    │
/// │                       ├────────────────►                    │
/// └───────────────────────┘                └────────────────────┘
pub trait StreamingDownloader {
    type Segment;

    // TODO: is this Vec necessary here?
    async fn fetch_info(&mut self) -> mpsc::UnboundedReceiver<Vec<Self::Segment>>;

    async fn fetch_segment(&self, segment: Self::Segment);
}

pub trait StreamingDownloaderExt: StreamingDownloader {
    async fn download(&mut self) {
        let mut info = self.fetch_info().await;
        while let Some(segments) = info.recv().await {
            for segment in segments {
                // FIXME: concurrency is limited to 1 here
                self.fetch_segment(segment).await;
            }
        }
    }
}

pub struct CommonM3u8ArchiveDownloader {
    m3u8: String,

    client: Arc<Client>,
}

impl CommonM3u8ArchiveDownloader {
    pub fn new(m3u8: String) -> Self {
        let client = Arc::new(Client::new());
        Self { client, m3u8 }
    }

    #[async_recursion::async_recursion]
    async fn load_m3u8(&self, url: Option<String>) -> (Url, MediaPlaylist) {
        log::info!("Start fetching M3U8 file.");

        let url = Url::from_str(&url.unwrap_or(self.m3u8.clone())).expect("Invalid URL");
        let m3u8_bytes = self
            .client
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
                    if let (Some(a), Some(b)) = (a.resolution, b.resolution) {
                        let resolution_cmp_result = a.width.cmp(&b.width);
                        if resolution_cmp_result != std::cmp::Ordering::Equal {
                            return resolution_cmp_result;
                        }
                    }
                    a.bandwidth.cmp(&b.bandwidth)
                });
                let variant = variants.get(0).expect("No variant found");
                let url = url.join(&variant.uri).expect("Invalid variant uri");

                log::debug!(
                    "Best stream: ${url}; Bandwidth: ${bandwidth}",
                    bandwidth = variant.bandwidth
                );
                self.load_m3u8(Some(url.to_string())).await
            }
            Ok(Playlist::MediaPlaylist(pl)) => (url, pl),
            Err(e) => panic!("Error: {:?}", e),
        }
    }
}

impl StreamingDownloader for CommonM3u8ArchiveDownloader {
    type Segment = M3u8Segment;

    async fn fetch_info(&mut self) -> mpsc::UnboundedReceiver<Vec<Self::Segment>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let (playlist_url, playlist) = self.load_m3u8(None).await;
        let _ = sender.send(
            playlist
                .segments
                .into_iter()
                .map(|s| M3u8Segment {
                    url: playlist_url.join(&s.uri).unwrap(),
                })
                .collect(),
        );
        receiver
    }

    async fn fetch_segment(&self, segment: Self::Segment) {
        let mut byte_stream = self
            .client
            .get(segment.url)
            .send()
            .await
            .expect("http error")
            .bytes_stream();
        let mut tmp_file = tokio::fs::File::create("/tmp/1.ts").await.unwrap();
        while let Some(item) = byte_stream.next().await {
            tokio::io::copy(&mut item.unwrap().as_ref(), &mut tmp_file)
                .await
                .unwrap();
        }
    }
}

impl StreamingDownloaderExt for CommonM3u8ArchiveDownloader {}

pub struct M3u8Segment {
    url: Url,
    // pub byte_range: Option<ByteRange>,
    // headers: HeaderMap,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_download() {
        let mut downloader = CommonM3u8ArchiveDownloader::new(
            "https://cph-p2p-msl.akamaized.net/hls/live/2000341/test/master.m3u8".to_string(),
        );
        downloader.download().await;
    }
}
