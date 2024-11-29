use std::sync::Arc;

use crate::{cache::CacheSource, error::IoriResult, merge::Merger, StreamingSource};

pub struct SequencialDownloader<S, M, C>
where
    S: StreamingSource,
    M: Merger,
    C: CacheSource,
{
    source: S,
    merger: M,
    cache: Arc<C>,
}

impl<S, M, C> SequencialDownloader<S, M, C>
where
    S: StreamingSource,
    M: Merger,
    C: CacheSource,
{
    pub fn new(source: S, merger: M, cache: C) -> Self {
        Self {
            source,
            merger,
            cache: Arc::new(cache),
        }
    }

    pub async fn download(&mut self) -> IoriResult<()> {
        let mut receiver = self.source.fetch_info().await?;

        while let Some(segment) = receiver.recv().await {
            for segment in segment? {
                let writer = self.cache.open_writer(&segment).await?;
                let Some(mut writer) = writer else {
                    continue;
                };

                let fetch_result = self.source.fetch_segment(&segment, &mut writer).await;
                drop(writer);

                match fetch_result {
                    Ok(_) => self.merger.update(segment, self.cache.clone()).await?,
                    Err(_) => self.merger.fail(segment, self.cache.clone()).await?,
                }
            }
        }

        self.merger.finish(self.cache.clone()).await?;
        Ok(())
    }
}
