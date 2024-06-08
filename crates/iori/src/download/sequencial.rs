use crate::{error::IoriResult, StreamingSource};

pub struct SequencialDownloader<S>
where
    S: StreamingSource,
{
    source: S,
}

impl<S> SequencialDownloader<S>
where
    S: StreamingSource,
{
    pub fn new(source: S) -> Self {
        Self { source }
    }

    pub async fn download(&mut self) -> IoriResult<Vec<S::SegmentInfo>> {
        let mut receiver = self.source.fetch_info().await?;

        let mut segments_info = Vec::new();
        while let Some(segment) = receiver.recv().await {
            for segment in segment? {
                if let Some(segment_info) = self.source.fetch_segment_info(&segment).await {
                    segments_info.push(segment_info);
                }

                self.source.fetch_segment(&segment, false).await?;
            }
        }

        Ok(segments_info)
    }
}
