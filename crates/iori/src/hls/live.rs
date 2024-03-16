use std::{path::PathBuf, sync::Arc};

use reqwest::Client;
use tokio::sync::mpsc;

use super::{
    core::{HlsSegmentFetcher, M3u8Source},
    M3u8Segment,
};
use crate::{consumer::Consumer, error::IoriResult, StreamingSource};

pub struct CommonM3u8LiveSource {
    playlist: Arc<M3u8Source>,
    segment: Arc<HlsSegmentFetcher>,
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
            segment: Arc::new(HlsSegmentFetcher::new(client, consumer)),
        }
    }
}

impl StreamingSource for CommonM3u8LiveSource {
    type Segment = M3u8Segment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let playlist = self.playlist.clone();
        tokio::spawn(async move {
            let mut latest_media_sequence = 0;
            loop {
                if sender.is_closed() {
                    break;
                }

                let (segments, _, playlist) = playlist
                    .load_segments(Some(latest_media_sequence))
                    .await
                    .unwrap();
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

    async fn fetch_segment(&self, segment: &Self::Segment) -> IoriResult<()> {
        self.segment.fetch(segment).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::SequencialDownloader;

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
