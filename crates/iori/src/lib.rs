pub mod cache;
pub mod dash;
pub mod decrypt;
pub mod download;
pub mod fetch;
pub mod hls;
pub mod merge;

mod error;
pub use error::*;

pub(crate) mod util;
pub use util::detect_manifest_type;

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
            tokio::sync::mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>,
        >,
    > + Send;

    fn fetch_segment<W>(
        &self,
        segment: &Self::Segment,
        writer: &mut W,
    ) -> impl std::future::Future<Output = IoriResult<()>> + Send
    where
        W: tokio::io::AsyncWrite + Unpin + Send + Sync + 'static;
}

pub trait StreamingSegment {
    /// Stream id
    fn stream(&self) -> u64;

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

    fn r#type(&self) -> SegmentType;
}

#[derive(Clone, Default)]
pub struct SegmentInfo {
    pub stream: u64,
    pub sequence: u64,
    pub file_name: String,
    pub initial_segment: Option<std::sync::Arc<Vec<u8>>>,
    pub key: Option<std::sync::Arc<decrypt::IoriKey>>,
    pub r#type: SegmentType,
}

impl SegmentInfo {
    pub fn index(&self) -> u128 {
        (self.stream as u128) << 64 | self.sequence as u128
    }
}

impl<T> From<&T> for SegmentInfo
where
    T: StreamingSegment,
{
    fn from(segment: &T) -> Self {
        SegmentInfo {
            stream: segment.stream(),
            sequence: segment.sequence(),
            file_name: segment.file_name().to_string(),
            initial_segment: segment.initial_segment(),
            key: segment.key(),
            r#type: segment.r#type(),
        }
    }
}

impl<'a> StreamingSegment for Box<dyn StreamingSegment + Send + Sync + 'a> {
    fn stream(&self) -> u64 {
        self.as_ref().stream()
    }

    fn sequence(&self) -> u64 {
        self.as_ref().sequence()
    }

    fn file_name(&self) -> &str {
        self.as_ref().file_name()
    }

    fn initial_segment(&self) -> Option<std::sync::Arc<Vec<u8>>> {
        self.as_ref().initial_segment()
    }

    fn key(&self) -> Option<std::sync::Arc<decrypt::IoriKey>> {
        self.as_ref().key()
    }

    fn r#type(&self) -> SegmentType {
        self.as_ref().r#type()
    }
}

impl<'a, 'b> StreamingSegment for &'a Box<dyn StreamingSegment + Send + Sync + 'b> {
    fn stream(&self) -> u64 {
        self.as_ref().stream()
    }

    fn sequence(&self) -> u64 {
        self.as_ref().sequence()
    }

    fn file_name(&self) -> &str {
        self.as_ref().file_name()
    }

    fn initial_segment(&self) -> Option<std::sync::Arc<Vec<u8>>> {
        self.as_ref().initial_segment()
    }

    fn key(&self) -> Option<std::sync::Arc<decrypt::IoriKey>> {
        self.as_ref().key()
    }

    fn r#type(&self) -> SegmentType {
        self.as_ref().r#type()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SegmentType {
    #[default]
    Video,
    Audio,
    Subtitle,
}

impl SegmentType {
    pub fn from_mime_type(mime_type: Option<&str>) -> Self {
        let mime_type = mime_type.unwrap_or("video");

        if mime_type.starts_with("video") {
            return Self::Video;
        } else if mime_type.starts_with("audio") {
            return Self::Audio;
        } else if mime_type.starts_with("text") {
            return Self::Subtitle;
        } else {
            panic!("Unknown mime type: {}", mime_type);
        }
    }
}

pub trait RemoteStreamingSegment {
    fn url(&self) -> reqwest::Url;

    fn byte_range(&self) -> Option<m3u8_rs::ByteRange> {
        None
    }
}

pub trait ToSegmentData {
    fn to_segment_data(
        &self,
        client: reqwest::Client,
    ) -> impl std::future::Future<Output = error::IoriResult<bytes::Bytes>> + Send;
}
