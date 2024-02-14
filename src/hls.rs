mod archive;
pub use archive::*;
use reqwest::Url;

pub struct M3u8Segment {
    url: Url,
    filename: String,
    key: Option<M3u8Aes128Key>,

    /// Sequence id allocated by the downloader
    sequence: u64,
    // pub byte_range: Option<ByteRange>,
    // headers: HeaderMap,
}

#[derive(Clone, Debug)]
pub struct M3u8Aes128Key {
    pub key: [u8; 16],
    pub iv: [u8; 16],
    pub keyformat: Option<String>,
    pub keyformatversions: Option<String>,
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
