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

    pub async fn download(&mut self) -> IoriResult<()> {
        let mut receiver = self.source.fetch_info().await?;

        while let Some(segment) = receiver.recv().await {
            for segment in segment {
                self.source.fetch_segment(&segment).await?;
            }
        }

        Ok(())
    }
}
