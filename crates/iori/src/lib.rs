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
pub use crate::util::http::HttpClient;
pub mod utils {
    pub use crate::util::detect_manifest_type;
    pub use crate::util::path::DuplicateOutputFileNamer;
}

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
    fn stream_id(&self) -> u64;

    /// Sequence ID of the segment, starts from 0
    fn sequence(&self) -> u64;

    /// File name of the segment
    fn file_name(&self) -> &str;

    /// Optional initial segment data
    fn initial_segment(&self) -> InitialSegment {
        InitialSegment::None
    }

    /// Optional key for decryption
    ///
    /// If a segment does not need to be decrypted, it must return `None` explicitly.
    fn key(&self) -> Option<std::sync::Arc<decrypt::IoriKey>>;

    fn r#type(&self) -> SegmentType;

    /// Format hint for the segment
    fn format(&self) -> SegmentFormat;
}

#[derive(Clone, Default)]
pub enum InitialSegment {
    Encrypted(std::sync::Arc<Vec<u8>>),
    Clear(std::sync::Arc<Vec<u8>>),
    #[default]
    None,
}

#[derive(Clone, Default)]
pub enum SegmentFormat {
    #[default]
    Mpeg2TS,
    Mp4,
    M4a,
    Cmfv,
    Cmfa,
    Other(String),
}

impl SegmentFormat {
    pub fn as_ext(&self) -> &str {
        match self {
            Self::Mpeg2TS => "ts",
            Self::Mp4 => "mp4",
            Self::M4a => "m4a",
            Self::Cmfv => "cmfv",
            Self::Cmfa => "cmfa",
            Self::Other(ext) => ext.as_str(),
        }
    }

    pub fn from_filename(s: &str) -> Self {
        let (_, ext) = s.rsplit_once('.').unwrap_or(("", s));
        match ext {
            "ts" => Self::Mpeg2TS,
            "mp4" | "m4s" | "m4f" => Self::Mp4,
            "m4a" => Self::M4a,
            "cmfv" => Self::Cmfv,
            "cmfa" => Self::Cmfa,
            _ => Self::Other(ext.to_string()),
        }
    }
}

#[derive(Clone, Default)]
pub struct SegmentInfo {
    pub stream_id: u64,
    pub sequence: u64,
    pub file_name: String,
    pub initial_segment: InitialSegment,
    pub key: Option<std::sync::Arc<decrypt::IoriKey>>,
    pub r#type: SegmentType,
    pub format: SegmentFormat,
}

impl<T> From<&T> for SegmentInfo
where
    T: StreamingSegment,
{
    fn from(segment: &T) -> Self {
        SegmentInfo {
            stream_id: segment.stream_id(),
            sequence: segment.sequence(),
            file_name: segment.file_name().to_string(),
            initial_segment: segment.initial_segment(),
            key: segment.key(),
            r#type: segment.r#type(),
            format: segment.format(),
        }
    }
}

impl<'a> StreamingSegment for Box<dyn StreamingSegment + Send + Sync + 'a> {
    fn stream_id(&self) -> u64 {
        self.as_ref().stream_id()
    }

    fn sequence(&self) -> u64 {
        self.as_ref().sequence()
    }

    fn file_name(&self) -> &str {
        self.as_ref().file_name()
    }

    fn initial_segment(&self) -> InitialSegment {
        self.as_ref().initial_segment()
    }

    fn key(&self) -> Option<std::sync::Arc<decrypt::IoriKey>> {
        self.as_ref().key()
    }

    fn r#type(&self) -> SegmentType {
        self.as_ref().r#type()
    }

    fn format(&self) -> SegmentFormat {
        self.as_ref().format()
    }
}

impl<'a, 'b> StreamingSegment for &'a Box<dyn StreamingSegment + Send + Sync + 'b> {
    fn stream_id(&self) -> u64 {
        self.as_ref().stream_id()
    }

    fn sequence(&self) -> u64 {
        self.as_ref().sequence()
    }

    fn file_name(&self) -> &str {
        self.as_ref().file_name()
    }

    fn initial_segment(&self) -> InitialSegment {
        self.as_ref().initial_segment()
    }

    fn key(&self) -> Option<std::sync::Arc<decrypt::IoriKey>> {
        self.as_ref().key()
    }

    fn r#type(&self) -> SegmentType {
        self.as_ref().r#type()
    }

    fn format(&self) -> SegmentFormat {
        self.as_ref().format()
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

    fn headers(&self) -> Option<reqwest::header::HeaderMap> {
        None
    }

    fn byte_range(&self) -> Option<m3u8_rs::ByteRange> {
        None
    }
}

pub trait ToSegmentData {
    fn to_segment_data(
        &self,
        client: util::http::HttpClient,
    ) -> impl std::future::Future<Output = error::IoriResult<bytes::Bytes>> + Send;
}
