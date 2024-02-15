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
    url: reqwest::Url,
    filename: String,

    key: Option<Arc<decrypt::M3u8Key>>,
    initial_segment: Option<Arc<Vec<u8>>>,

    byte_range: Option<m3u8_rs::ByteRange>,

    /// Sequence id allocated by the downloader
    sequence: u64,
    /// Media sequence id from the m3u8 file
    media_sequence: u64,
}

impl crate::StreamingSegment for M3u8Segment {
    fn file_name(&self) -> &str {
        self.filename.as_str()
    }
}
