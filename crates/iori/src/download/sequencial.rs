use crate::{error::IoriResult, merge::Merger, StreamingSource};

pub struct SequencialDownloader<S, M>
where
    S: StreamingSource,
    M: Merger<Segment = S::Segment>,
{
    source: S,
    merger: M,
}

impl<S, M> SequencialDownloader<S, M>
where
    S: StreamingSource,
    M: Merger<Segment = S::Segment>,
{
    pub fn new(source: S, merger: M) -> Self {
        Self { source, merger }
    }

    pub async fn download(&mut self) -> IoriResult<()> {
        let mut receiver = self.source.fetch_info().await?;

        while let Some(segment) = receiver.recv().await {
            for segment in segment? {
                let merge_segment = self.merger.open_writer(&segment).await?;
                let Some(mut merge_segment) = merge_segment else {
                    continue;
                };

                let fetch_result = self
                    .source
                    .fetch_segment(&segment, &mut merge_segment)
                    .await;

                match fetch_result {
                    Ok(_) => self.merger.update(segment).await?,
                    Err(_) => self.merger.fail(segment).await?,
                }
            }
        }

        Ok(())
    }
}
