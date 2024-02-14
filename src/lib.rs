pub mod downloader;
pub mod error;
pub mod hls;

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
    type Segment: StreamingSegment + Send + 'static;

    // TODO: maybe this method can be sync?
    fn fetch_info(
        &mut self,
    ) -> impl std::future::Future<
        Output = error::IoriResult<tokio::sync::mpsc::UnboundedReceiver<Vec<Self::Segment>>>,
    > + Send;

    fn fetch_segment(
        &self,
        segment: Self::Segment,
    ) -> impl std::future::Future<Output = error::IoriResult<Self::Segment>> + Send;
}

pub trait StreamingSegment {
    fn file_name(&self) -> &str;
}
