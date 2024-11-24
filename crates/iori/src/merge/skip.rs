use super::Merger;
use crate::{cache::CacheSource, error::IoriResult, StreamingSegment};

pub struct SkipMerger;

impl SkipMerger {
    pub fn new() -> Self {
        Self
    }
}

impl Merger for SkipMerger {
    type Result = ();

    async fn update(
        &mut self,
        _segment: impl StreamingSegment,
        _cache: &impl CacheSource,
    ) -> IoriResult<()> {
        Ok(())
    }

    async fn fail(
        &mut self,
        _segment: impl StreamingSegment,
        _cache: &impl CacheSource,
    ) -> IoriResult<()> {
        Ok(())
    }

    async fn finish(&mut self, cache: &impl CacheSource) -> IoriResult<Self::Result> {
        log::info!("Skip merging. Please merge video chunks manually.");
        log::info!("Temporary files are located at {:?}", cache.location_hint());
        Ok(())
    }
}
