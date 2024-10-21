use std::{path::PathBuf, sync::Arc, time::Duration};

use reqwest::Client;
use tokio::{io::AsyncWrite, sync::mpsc};

use crate::{
    common::fetch_segment,
    error::{IoriError, IoriResult},
    hls::{segment::M3u8Segment, source::M3u8Source},
    StreamingSource,
};

pub struct CommonM3u8LiveSource {
    client: Client,
    playlist: Arc<M3u8Source>,
    retry: u32,
}

impl CommonM3u8LiveSource {
    pub fn new(
        client: Client,
        m3u8: String,
        key: Option<String>,
        shaka_packager_command: Option<PathBuf>,
    ) -> Self {
        Self {
            client: client.clone(),
            playlist: Arc::new(M3u8Source::new(client, m3u8, key, shaka_packager_command)),
            retry: 3,
        }
    }

    pub fn with_retry(mut self, retry: u32) -> Self {
        self.retry = retry;
        self
    }
}

impl StreamingSource for CommonM3u8LiveSource {
    type Segment = M3u8Segment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let retry = self.retry;
        let playlist = self.playlist.clone();
        tokio::spawn(async move {
            let mut latest_media_sequence = 0;
            loop {
                if sender.is_closed() {
                    break;
                }

                let before_load = tokio::time::Instant::now();
                let (segments, _, playlist) = match playlist
                    .load_segments(Some(latest_media_sequence), retry)
                    .await
                {
                    Ok(v) => v,
                    Err(IoriError::M3u8FetchError) => {
                        log::error!("Exceeded retry limit for fetching segments, exiting...");
                        break;
                    }
                    Err(e) => {
                        log::error!("Failed to fetch segments: {e}");
                        break;
                    }
                };
                let new_latest_media_sequence = segments
                    .last()
                    .map(|r| r.media_sequence)
                    .unwrap_or(latest_media_sequence);

                if let Err(_) = sender.send(Ok(segments)) {
                    break;
                }
                latest_media_sequence = new_latest_media_sequence;

                if playlist.end_list {
                    break;
                }

                let segment_average_duration =
                    (playlist.segments.iter().map(|s| s.duration).sum::<f32>()
                        / playlist.segments.len() as f32) as u64;

                // playlist does not end, wait for a while and fetch again
                let seconds_to_wait = segment_average_duration.min(5);
                tokio::time::sleep_until(before_load + Duration::from_secs(seconds_to_wait)).await;
            }
        });

        Ok(receiver)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        fetch_segment(self.client.clone(), segment, writer).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cache::file::FileCacheSource, download::SequencialDownloader, merge::SkipMerger};

    #[tokio::test]
    async fn test_download_live() -> IoriResult<()> {
        let source = CommonM3u8LiveSource::new(
            Default::default(),
            "https://cph-p2p-msl.akamaized.net/hls/live/2000341/test/master.m3u8".to_string(),
            None,
            None,
        );
        let merger = SkipMerger::new("/tmp/test");
        let cache = FileCacheSource::new("/tmp/test".into());
        SequencialDownloader::new(source, merger, cache)
            .download()
            .await?;

        Ok(())
    }
}
