#![allow(async_fn_in_trait)]
pub mod hls;

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
