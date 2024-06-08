pub mod common;
pub mod consumer;
pub mod dash;
pub mod decrypt;
pub mod download;
pub mod error;
pub mod hls;
pub mod merge;
pub mod util;

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
    type SegmentInfo: serde::Serialize + serde::de::DeserializeOwned + Send + 'static;

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

    fn fetch_segment_info(
        &self,
        _segment: &Self::Segment,
    ) -> impl std::future::Future<Output = Option<Self::SegmentInfo>> + Send {
        async move { None }
    }
}

pub trait StreamingSegment {
    /// Sequence ID of the segment, starts from 0
    fn sequence(&self) -> u64;

    /// File name of the segment
    fn file_name(&self) -> &str;

    /// Optional initial segment data
    fn initial_segment(&self) -> Option<std::sync::Arc<Vec<u8>>> {
        None
    }

    /// Optional key for decryption
    ///
    /// If a segment does not need to be decrypted, it must return `None` explicitly.
    fn key(&self) -> Option<std::sync::Arc<decrypt::IoriKey>>;
}

pub trait RemoteStreamingSegment {
    fn url(&self) -> reqwest::Url;

    fn byte_range(&self) -> Option<m3u8_rs::ByteRange> {
        None
    }
}

pub trait ToSegmentData {
    fn to_segment(
        &self,
        client: std::sync::Arc<reqwest::Client>,
    ) -> impl std::future::Future<Output = error::IoriResult<bytes::Bytes>> + Send;
}
