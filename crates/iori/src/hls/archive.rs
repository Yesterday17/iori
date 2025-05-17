use std::{num::ParseIntError, path::PathBuf, str::FromStr, sync::Arc};

use tokio::{
    io::AsyncWrite,
    sync::{mpsc, Mutex},
};
use url::Url;

use crate::{
    error::IoriResult,
    fetch::fetch_segment,
    hls::{segment::M3u8Segment, source::AdvancedM3u8Source},
    util::http::HttpClient,
    StreamingSource,
};

pub struct CommonM3u8ArchiveSource {
    client: HttpClient,
    playlist: Arc<Mutex<AdvancedM3u8Source>>,
    range: SegmentRange,
    retry: u32,
    shaka_packager_command: Option<PathBuf>,
}

/// A subrange for m3u8 archive sources to choose which segment to use
#[derive(Debug, Clone, Copy)]
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
        client: HttpClient,
        playlist_url: String,
        key: Option<&str>,
        range: SegmentRange,
        shaka_packager_command: Option<PathBuf>,
    ) -> Self {
        Self {
            client: client.clone(),
            playlist: Arc::new(Mutex::new(AdvancedM3u8Source::new(
                client,
                Url::parse(&playlist_url).unwrap(),
                key,
            ))),
            shaka_packager_command,
            range,
            retry: 3,
        }
    }

    pub fn with_retry(mut self, retry: u32) -> Self {
        self.retry = retry;
        self
    }
}

impl StreamingSource for CommonM3u8ArchiveSource {
    type Segment = M3u8Segment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let latest_media_sequences = self.playlist.lock().await.load_streams(self.retry).await?;

        let (sender, receiver) = mpsc::unbounded_channel();

        let (segments, _) = self
            .playlist
            .lock()
            .await
            .load_segments(&latest_media_sequences, self.retry)
            .await?;
        let mut segments: Vec<_> = segments
            .into_iter()
            .flatten()
            .filter_map(|segment| {
                let seq = segment.sequence + 1;
                if seq >= self.range.start && seq <= self.range.end() {
                    return Some(segment);
                }
                None
            })
            .collect();

        // make sequence start form 1 again
        let mut seq = 0;
        for segment in segments.iter_mut() {
            segment.sequence = seq;
            seq += 1;
        }

        let _ = sender.send(Ok(segments));

        Ok(receiver)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        fetch_segment(
            self.client.clone(),
            segment,
            writer,
            self.shaka_packager_command.clone(),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
