use std::{path::PathBuf, sync::Arc};

use tokio::sync::mpsc;

use super::{CommonM3u8ArchiveSource, M3u8Segment};
use crate::StreamingSource;

pub struct CommonM3u8LiveSource {
    inner: Arc<CommonM3u8ArchiveSource>,
}

impl CommonM3u8LiveSource {
    pub fn new(m3u8: String, output_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(CommonM3u8ArchiveSource::new(m3u8, output_dir)),
        }
    }
}

impl StreamingSource for CommonM3u8LiveSource {
    type Segment = M3u8Segment;

    async fn fetch_info(&mut self) -> mpsc::UnboundedReceiver<Self::Segment> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let inner: Arc<CommonM3u8ArchiveSource> = self.inner.clone();
        tokio::spawn(async move {
            let mut latest_media_sequence = 0;
            loop {
                if sender.is_closed() {
                    break;
                }

                let (segments, _, playlist) =
                    inner.load_segments(Some(latest_media_sequence)).await;
                let new_latest_media_sequence = segments
                    .last()
                    .map(|r| r.media_sequence)
                    .unwrap_or(latest_media_sequence);

                for segment in segments {
                    eprintln!("LIVE SEGMENT #{:06}: {}", segment.sequence, segment.url);
                    if let Err(_) = sender.send(segment) {
                        break;
                    }
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

        receiver
    }

    async fn fetch_segment(&self, segment: Self::Segment) {
        self.inner.fetch_segment(segment).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::SequencialDownloader;

    #[tokio::test]
    async fn test_download_live() {
        let source = CommonM3u8LiveSource::new(
            "https://cph-p2p-msl.akamaized.net/hls/live/2000341/test/master.m3u8".to_string(),
            "/tmp/test_live".into(),
        );
        SequencialDownloader::new(source).download().await;
    }
}