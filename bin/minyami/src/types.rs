use iori::{
    cache::{
        file::FileCacheSource, memory::MemoryCacheSource, CacheSource, CacheSourceReader,
        CacheSourceWriter,
    },
    IoriResult, StreamingSegment,
};

pub enum MinyamiCache {
    Memory(MemoryCacheSource),
    File(FileCacheSource),
}

impl CacheSource for MinyamiCache {
    async fn open_writer(
        &self,
        segment: &impl StreamingSegment,
    ) -> IoriResult<Option<CacheSourceWriter>> {
        match self {
            MinyamiCache::Memory(cache) => cache.open_writer(segment).await,
            MinyamiCache::File(cache) => cache.open_writer(segment).await,
        }
    }

    async fn open_reader(&self, segment: &impl StreamingSegment) -> IoriResult<CacheSourceReader> {
        match self {
            MinyamiCache::Memory(cache) => cache.open_reader(segment).await,
            MinyamiCache::File(cache) => cache.open_reader(segment).await,
        }
    }

    async fn segment_path(&self, segment: &impl StreamingSegment) -> Option<std::path::PathBuf> {
        match self {
            MinyamiCache::Memory(cache) => cache.segment_path(segment).await,
            MinyamiCache::File(cache) => cache.segment_path(segment).await,
        }
    }

    async fn invalidate(&self, segment: &impl StreamingSegment) -> IoriResult<()> {
        match self {
            MinyamiCache::Memory(cache) => cache.invalidate(segment).await,
            MinyamiCache::File(cache) => cache.invalidate(segment).await,
        }
    }

    async fn clear(&self) -> IoriResult<()> {
        match self {
            MinyamiCache::Memory(cache) => cache.clear().await,
            MinyamiCache::File(cache) => cache.clear().await,
        }
    }

    fn location_hint(&self) -> Option<String> {
        match self {
            MinyamiCache::Memory(cache) => cache.location_hint(),
            MinyamiCache::File(cache) => cache.location_hint(),
        }
    }
}
