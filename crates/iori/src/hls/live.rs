use std::{path::PathBuf, sync::Arc};

use reqwest::Client;
use tokio::sync::mpsc;

use crate::{
    common::CommonSegmentFetcher,
    consumer::Consumer,
    error::{IoriError, IoriResult},
    hls::{
        segment::{M3u8Segment, M3u8SegmentInfo},
        source::M3u8Source,
    },
    StreamingSource,
};

pub struct CommonM3u8LiveSource {
    playlist: Arc<M3u8Source>,
    segment: Arc<CommonSegmentFetcher>,
    retry: u32,
}

impl CommonM3u8LiveSource {
    pub fn new(
        client: Client,
        m3u8: String,
        key: Option<String>,
        consumer: Consumer,
        shaka_packager_command: Option<PathBuf>,
    ) -> Self {
        let client = Arc::new(client);
        Self {
            playlist: Arc::new(M3u8Source::new(
                client.clone(),
                m3u8,
                key,
                shaka_packager_command,
            )),
            segment: Arc::new(CommonSegmentFetcher::new(client, consumer)),
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
    type SegmentInfo = M3u8SegmentInfo;

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
                tokio::time::sleep(std::time::Duration::from_secs(
                    segment_average_duration.min(5),
                ))
                .await;
            }
        });

        Ok(receiver)
    }

    async fn fetch_segment(&self, segment: &Self::Segment, will_retry: bool) -> IoriResult<()> {
        self.segment.fetch(segment, will_retry).await
    }

    async fn fetch_segment_info(&self, segment: &Self::Segment) -> Option<Self::SegmentInfo> {
        Some(Self::SegmentInfo {
            url: segment.url.clone(),
            filename: segment.filename.clone(),
            sequence: segment.sequence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::SequencialDownloader;

    #[tokio::test]
    async fn test_download_live() -> IoriResult<()> {
        let source = CommonM3u8LiveSource::new(
            Default::default(),
            "https://cph-p2p-msl.akamaized.net/hls/live/2000341/test/master.m3u8".to_string(),
            None,
            Consumer::file("/tmp/test_live")?,
            None,
        );
        SequencialDownloader::new(source).download().await?;

        Ok(())
    }
}
