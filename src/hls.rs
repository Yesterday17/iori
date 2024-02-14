mod archive;
mod decrypt;
mod live;
mod utils;

pub use archive::CommonM3u8ArchiveDownloader;
pub use live::CommonM3u8LiveDownloader;

use self::decrypt::M3u8Key;

pub struct M3u8Segment {
    url: reqwest::Url,
    filename: String,
    key: Option<M3u8Key>,

    /// Sequence id allocated by the downloader
    sequence: u64,
    // pub byte_range: Option<ByteRange>,
}
