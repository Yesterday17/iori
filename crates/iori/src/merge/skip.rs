use super::Merger;
use crate::{cache::CacheSource, error::IoriResult, StreamingSegment};
use std::marker::PhantomData;

pub struct SkipMerger<S> {
    _phantom: PhantomData<S>,
}

impl<S> SkipMerger<S> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<S> Merger for SkipMerger<S>
where
    S: StreamingSegment + Send + 'static,
{
    type Segment = S;
    type Result = ();

    async fn update(
        &mut self,
        _segment: Self::Segment,
        _cache: &impl CacheSource,
    ) -> IoriResult<()> {
        Ok(())
    }

    async fn fail(&mut self, _segment: Self::Segment, _cache: &impl CacheSource) -> IoriResult<()> {
        Ok(())
    }

    async fn finish(&mut self, cache: &impl CacheSource) -> IoriResult<Self::Result> {
        log::info!("Skip merging. Please merge video chunks manually.");
        log::info!("Temporary files are located at {:?}", cache.location_hint());
        Ok(())
    }
}
