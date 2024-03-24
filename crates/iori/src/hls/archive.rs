use std::{num::ParseIntError, path::PathBuf, str::FromStr, sync::Arc};

use reqwest::Client;
use tokio::sync::mpsc;

use super::{
    core::{HlsSegmentFetcher, M3u8Source},
    M3u8Segment,
};
use crate::{consumer::Consumer, error::IoriResult, StreamingSource};

pub struct CommonM3u8ArchiveSource {
    playlist: Arc<M3u8Source>,
    segment: Arc<HlsSegmentFetcher>,
    range: SegmentRange,
}

/// A subrange for m3u8 archive sources to choose which segment to use
#[derive(Debug, Clone)]
pub struct SegmentRange {
    /// Start offset to use. Default to 1
    pub start: u64,
    /// End offset to use. Default to None
    pub end: Option<u64>,
}

impl Default for SegmentRange {
    fn default() -> Self {
        Self {
            start: 1,
            end: None,
        }
    }
}

impl SegmentRange {
    pub fn new(start: u64, end: Option<u64>) -> Self {
        Self { start, end }
    }

    pub fn end(&self) -> u64 {
        self.end.unwrap_or(std::u64::MAX)
    }
}

impl FromStr for SegmentRange {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (start, end) = s.split_once('-').unwrap_or((s, ""));
        let start = if start.is_empty() { 1 } else { start.parse()? };
        let end = if end.is_empty() {
            None
        } else {
            Some(end.parse()?)
        };
        Ok(Self { start, end })
    }
}

impl CommonM3u8ArchiveSource {
    pub fn new(
        client: Client,
        m3u8: String,
        key: Option<String>,
        range: SegmentRange,
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
            range,
        }
    }
}

impl StreamingSource for CommonM3u8ArchiveSource {
    type Segment = M3u8Segment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let (segments, _, _) = self.playlist.load_segments(None).await?;
        let segments = segments
            .into_iter()
            .filter_map(|segment| {
                let seq = segment.sequence + 1;
                if seq >= self.range.start && seq <= self.range.end() {
                    return Some(segment);
                }
                None
            })
            .collect();
        let _ = sender.send(Ok(segments));

        Ok(receiver)
    }

    async fn fetch_segment(&self, segment: &Self::Segment, will_retry: bool) -> IoriResult<()> {
        self.segment.fetch(segment, will_retry).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::SequencialDownloader;

    #[tokio::test]
    async fn test_download_archive() -> IoriResult<()> {
        let source = CommonM3u8ArchiveSource::new(
            Default::default(),
            "https://test-streams.mux.dev/bbbAES/playlists/sample_aes/index.m3u8".to_string(),
            None,
            Default::default(),
            Consumer::file("/tmp/test")?,
            None,
        );
        SequencialDownloader::new(source).download().await?;

        Ok(())
    }

    #[test]
    fn test_parse_range() {
        let range = "1-10".parse::<SegmentRange>().unwrap();
        assert_eq!(range.start, 1);
        assert_eq!(range.end, Some(10));

        let range = "1-".parse::<SegmentRange>().unwrap();
        assert_eq!(range.start, 1);
        assert_eq!(range.end, None);

        let range = "-10".parse::<SegmentRange>().unwrap();
        assert_eq!(range.start, 1);
        assert_eq!(range.end, Some(10));

        let range = "1".parse::<SegmentRange>().unwrap();
        assert_eq!(range.start, 1);
        assert_eq!(range.end, None);
    }
}
