use reqwest::{header::HeaderMap, Url};
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
    type Segment: Clone;

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

struct CommonM3u8Downloader {
    //
}

impl StreamingDownloader for CommonM3u8Downloader {
    type Segment = M3u8Segment;

    async fn fetch_info(&mut self) -> mpsc::UnboundedReceiver<Vec<Self::Segment>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        receiver
    }

    async fn fetch_segment(&self, segment: Self::Segment) {
        //
    }
}

impl StreamingDownloaderExt for CommonM3u8Downloader {}

#[derive(Clone)]
struct M3u8Segment {
    url: Url,
    headers: HeaderMap,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_download() {
        let mut downloader = CommonM3u8Downloader {};
        downloader.download().await;
    }
}
