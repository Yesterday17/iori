use super::Merger;
use crate::{cache::CacheSource, error::IoriResult, SegmentInfo};

pub struct SkipMerger;

impl Merger for SkipMerger {
    type Result = ();

    async fn update(&mut self, _segment: SegmentInfo, _cache: impl CacheSource) -> IoriResult<()> {
        Ok(())
    }

    async fn fail(&mut self, _segment: SegmentInfo, _cache: impl CacheSource) -> IoriResult<()> {
        Ok(())
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
        tracing::info!("Skip merging. Please merge video chunks manually.");
        tracing::info!("Temporary files are located at {:?}", cache.location_hint());
        Ok(())
    }
}
