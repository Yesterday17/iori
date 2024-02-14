mod archive;
mod decrypt;
mod live;
mod utils;

pub use archive::CommonM3u8ArchiveDownloader;

use self::decrypt::M3u8Aes128Key;

pub struct M3u8Segment {
    url: reqwest::Url,
    filename: String,
    key: Option<M3u8Aes128Key>,

    /// Sequence id allocated by the downloader
    sequence: u64,
    // pub byte_range: Option<ByteRange>,
}

#[cfg(test)]
mod tests {
    use crate::StreamingDownloaderExt;

    use super::*;

    #[tokio::test]
    async fn test_download() {
        let mut downloader = CommonM3u8ArchiveDownloader::new(
            "https://test-streams.mux.dev/bbbAES/playlists/sample_aes/index.m3u8".to_string(),
            "/tmp/test".into(),
        );
        downloader.download().await;
    }
}
