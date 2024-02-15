use std::{path::PathBuf, sync::Arc};

use reqwest::Client;
use tokio::sync::mpsc;

use super::{core::M3u8ListSource, M3u8Segment};
use crate::{error::IoriResult, StreamingSource};

pub struct CommonM3u8ArchiveSource {
    inner: Arc<M3u8ListSource>,
}

impl CommonM3u8ArchiveSource {
    pub fn new(
        client: Client,
        m3u8: String,
        key: Option<String>,
        output_dir: PathBuf,
        shaka_packager_command: Option<PathBuf>,
    ) -> Self {
        Self {
            inner: Arc::new(M3u8ListSource::new(
                client,
                m3u8,
                key,
                output_dir,
                shaka_packager_command,
            )),
        }
    }
}

impl StreamingSource for CommonM3u8ArchiveSource {
    type Segment = M3u8Segment;

    async fn fetch_info(&mut self) -> IoriResult<mpsc::UnboundedReceiver<Vec<Self::Segment>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let (segments, _, _) = self.inner.load_segments(None).await?;
        let _ = sender.send(segments);

        Ok(receiver)
    }

    async fn fetch_segment(&self, segment: &Self::Segment) -> IoriResult<()> {
        self.inner.fetch_segment(segment).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::SequencialDownloader;

    #[tokio::test]
    async fn test_download_archive() -> IoriResult<()> {
        let source = CommonM3u8ArchiveSource::new(
            Default::default(),
            "https://test-streams.mux.dev/bbbAES/playlists/sample_aes/index.m3u8".to_string(),
            None,
            "/tmp/test".into(),
            None,
        );
        SequencialDownloader::new(source).download().await?;

        Ok(())
    }
}
