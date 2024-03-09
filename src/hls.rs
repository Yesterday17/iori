mod archive;
mod core;
mod decrypt;
mod live;
mod utils;

use std::sync::Arc;

pub use archive::{CommonM3u8ArchiveSource, SegmentRange};
pub use core::M3u8Source;
pub use decrypt::M3u8Key;
pub use live::CommonM3u8LiveSource;
pub use m3u8_rs;

use crate::StreamingSegment;

pub struct M3u8Segment {
    pub url: reqwest::Url,
    pub filename: String,

    pub key: Option<Arc<decrypt::M3u8Key>>,
    pub initial_segment: Option<Arc<Vec<u8>>>,

    pub byte_range: Option<m3u8_rs::ByteRange>,

    /// Sequence id allocated by the downloader, starts from 0
    pub sequence: u64,
    /// Media sequence id from the m3u8 file
    pub media_sequence: u64,
}

pub trait M3u8StreamingSegment: StreamingSegment {
    fn url(&self) -> reqwest::Url;
    fn key(&self) -> Option<Arc<decrypt::M3u8Key>>;
    fn initial_segment(&self) -> Option<Arc<Vec<u8>>>;
    fn byte_range(&self) -> Option<m3u8_rs::ByteRange>;
    fn media_sequence(&self) -> u64;
}

impl crate::StreamingSegment for M3u8Segment {
    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn file_name(&self) -> &str {
        self.filename.as_str()
    }
}

impl M3u8StreamingSegment for M3u8Segment {
    fn url(&self) -> reqwest::Url {
        self.url.clone()
    }

    fn key(&self) -> Option<Arc<decrypt::M3u8Key>> {
        self.key.clone()
    }

    fn initial_segment(&self) -> Option<Arc<Vec<u8>>> {
        self.initial_segment.clone()
    }

    fn byte_range(&self) -> Option<m3u8_rs::ByteRange> {
        self.byte_range.clone()
    }

    fn media_sequence(&self) -> u64 {
        self.media_sequence
    }
}
