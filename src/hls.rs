mod archive;
mod decrypt;
mod live;
mod utils;

use std::sync::Arc;

pub use archive::CommonM3u8ArchiveSource;
pub use live::CommonM3u8LiveSource;

use crate::StreamingSegment;

use self::decrypt::M3u8Key;

pub struct M3u8Segment {
    url: reqwest::Url,
    filename: String,

    key: Option<Arc<M3u8Key>>,
    initial_segment: Option<Arc<Vec<u8>>>,

    /// Sequence id allocated by the downloader
    sequence: u64,
    /// Media sequence id from the m3u8 file
    media_sequence: u64,
    // pub byte_range: Option<ByteRange>,
}

impl StreamingSegment for M3u8Segment {
    fn file_name(&self) -> &str {
        self.filename.as_str()
    }
}
