mod archive;
mod decrypt;
mod live;
mod utils;

use std::hash::Hash;

pub use archive::CommonM3u8ArchiveSource;
pub use live::CommonM3u8LiveSource;

use self::decrypt::M3u8Key;

pub struct M3u8Segment {
    url: reqwest::Url,
    filename: String,
    key: Option<M3u8Key>,

    /// Sequence id allocated by the downloader
    sequence: u64,
    media_sequence: u64,
    // pub byte_range: Option<ByteRange>,
}

impl Hash for M3u8Segment {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // TODO: add byte range to hash
        self.url.hash(state);
    }
}
