#![allow(async_fn_in_trait)]
use std::{
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use futures_util::StreamExt;
use m3u8_rs::{MediaPlaylist, Playlist};
use reqwest::{Client, Url};
use tokio::{fs::File, sync::mpsc};

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
pub trait StreamingSource {
    type Segment;

    // TODO: maybe this method can be sync?
    async fn fetch_info(&mut self) -> mpsc::UnboundedReceiver<Self::Segment>;

    async fn fetch_segment(&self, segment: Self::Segment);
}

// TODO: maybe this should not be a trait?
pub trait StreamingDownloaderExt: StreamingSource {
    async fn download(&mut self) {
        let mut info = self.fetch_info().await;
        while let Some(segment) = info.recv().await {
            // FIXME: concurrency is limited to 1 here
            self.fetch_segment(segment).await;
        }
    }
}

pub struct CommonM3u8ArchiveDownloader {
    m3u8_url: String,

    output_dir: PathBuf,
    sequence: AtomicU64,
    client: Arc<Client>,
}

impl CommonM3u8ArchiveDownloader {
    pub fn new(m3u8: String, output_dir: PathBuf) -> Self {
        let client = Arc::new(Client::new());
        Self {
            m3u8_url: m3u8,
            output_dir,

            sequence: AtomicU64::new(0),
            client,
        }
    }

    #[async_recursion::async_recursion]
    async fn load_m3u8(&self, url: Option<String>) -> (Url, MediaPlaylist) {
        log::info!("Start fetching M3U8 file.");

        let url = Url::from_str(&url.unwrap_or(self.m3u8_url.clone())).expect("Invalid URL");
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

impl StreamingSource for CommonM3u8ArchiveDownloader {
    type Segment = M3u8Segment;

    async fn fetch_info(&mut self) -> mpsc::UnboundedReceiver<Self::Segment> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let (playlist_url, playlist) = self.load_m3u8(None).await;

        for segment in playlist.segments {
            let url = playlist_url.join(&segment.uri).unwrap();
            // FIXME: filename may be too long
            let filename = url
                .path_segments()
                .and_then(|c| c.last())
                .unwrap_or("output.ts")
                .to_string();
            let segment = M3u8Segment {
                url,
                filename,
                sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
            };
            if let Err(_) = sender.send(segment) {
                break;
            }
        }
        receiver
    }

    async fn fetch_segment(&self, segment: Self::Segment) {
        if !self.output_dir.exists() {
            tokio::fs::create_dir_all(&self.output_dir).await.unwrap();
        }

        let filename = segment.filename;
        let sequence = segment.sequence;
        let mut tmp_file = File::create(self.output_dir.join(format!("{sequence:06}_{filename}")))
            .await
            .unwrap();

        let mut byte_stream = self
            .client
            .get(segment.url)
            .send()
            .await
            .expect("http error")
            .bytes_stream();
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
    filename: String,
    sequence: u64,
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
            "/tmp/test".into(),
        );
        downloader.download().await;
    }
}
