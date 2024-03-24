pub mod consumer;
pub mod download;
pub mod error;
pub mod hls;
pub mod merge;

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
        &self,
    ) -> impl std::future::Future<
        Output = error::IoriResult<
            tokio::sync::mpsc::UnboundedReceiver<error::IoriResult<Vec<Self::Segment>>>,
        >,
    > + Send;

    fn fetch_segment(
        &self,
        segment: &Self::Segment,
        will_retry: bool,
    ) -> impl std::future::Future<Output = error::IoriResult<()>> + Send;
}

pub trait StreamingSegment {
    fn sequence(&self) -> u64;

    fn file_name(&self) -> &str;
}
