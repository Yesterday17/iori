mod archive;
mod core;
mod decrypt;
mod live;
mod utils;

use std::sync::Arc;

pub use archive::CommonM3u8ArchiveSource;
pub use core::M3u8ListSource;
pub use live::CommonM3u8LiveSource;

pub struct M3u8Segment {
    pub url: reqwest::Url,
    pub filename: String,

    pub key: Option<Arc<decrypt::M3u8Key>>,
    pub initial_segment: Option<Arc<Vec<u8>>>,

    pub byte_range: Option<m3u8_rs::ByteRange>,

    /// Sequence id allocated by the downloader
    pub sequence: u64,
    /// Media sequence id from the m3u8 file
    pub media_sequence: u64,
}

impl crate::StreamingSegment for M3u8Segment {
    fn file_name(&self) -> &str {
        self.filename.as_str()
    }
}
