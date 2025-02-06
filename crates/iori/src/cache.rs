pub mod file;
pub mod memory;

use crate::{error::IoriResult, SegmentInfo};
use std::{future::Future, path::PathBuf, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};

pub type CacheSourceReader = Box<dyn AsyncRead + Unpin + Send + Sync + 'static>;
pub type CacheSourceWriter = Box<dyn AsyncWrite + Unpin + Send + Sync + 'static>;

/// A cache source for storing the downloaded but not merged segments.
pub trait CacheSource: Send + Sync + 'static {
    /// Open a writer for writing data of the segment.
    fn open_writer(
        &self,
        segment: &SegmentInfo,
    ) -> impl Future<Output = IoriResult<Option<CacheSourceWriter>>> + Send;

    /// Open a reader for reading data of the segment.
    fn open_reader(
        &self,
        segment: &SegmentInfo,
    ) -> impl Future<Output = IoriResult<CacheSourceReader>> + Send;

    /// Get stored location of the segment
    fn segment_path(&self, _segment: &SegmentInfo) -> impl Future<Output = Option<PathBuf>> + Send {
        async { None }
    }

    /// Invalidate the cache of the segment from the cache source.
    fn invalidate(&self, segment: &SegmentInfo) -> impl Future<Output = IoriResult<()>> + Send;

    /// Clear the cache source.
    fn clear(&self) -> impl Future<Output = IoriResult<()>> + Send;

    /// Hint a location for the cached segments.
    fn location_hint(&self) -> Option<String> {
        None
    }
}

impl<C> CacheSource for Arc<C>
where
    C: CacheSource + Send,
{
    fn open_writer(
        &self,
        segment: &SegmentInfo,
    ) -> impl Future<Output = IoriResult<Option<CacheSourceWriter>>> + Send {
        self.as_ref().open_writer(segment)
    }

    fn open_reader(
        &self,
        segment: &SegmentInfo,
    ) -> impl Future<Output = IoriResult<CacheSourceReader>> + Send {
        self.as_ref().open_reader(segment)
    }

    fn segment_path(&self, segment: &SegmentInfo) -> impl Future<Output = Option<PathBuf>> + Send {
        self.as_ref().segment_path(segment)
    }

    fn invalidate(&self, segment: &SegmentInfo) -> impl Future<Output = IoriResult<()>> + Send {
        self.as_ref().invalidate(segment)
    }

    fn clear(&self) -> impl Future<Output = IoriResult<()>> {
        self.as_ref().clear()
    }

    fn location_hint(&self) -> Option<String> {
        self.as_ref().location_hint()
    }
}

pub enum IoriCache {
    Memory(memory::MemoryCacheSource),
    File(file::FileCacheSource),
}

impl IoriCache {
    pub fn memory() -> Self {
        Self::Memory(memory::MemoryCacheSource::new())
    }

    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(file::FileCacheSource::new(path.into()))
    }
}

impl CacheSource for IoriCache {
    async fn open_writer(&self, segment: &SegmentInfo) -> IoriResult<Option<CacheSourceWriter>> {
        match self {
            IoriCache::Memory(cache) => cache.open_writer(segment).await,
            IoriCache::File(cache) => cache.open_writer(segment).await,
        }
    }

    async fn open_reader(&self, segment: &SegmentInfo) -> IoriResult<CacheSourceReader> {
        match self {
            IoriCache::Memory(cache) => cache.open_reader(segment).await,
            IoriCache::File(cache) => cache.open_reader(segment).await,
        }
    }

    async fn segment_path(&self, segment: &SegmentInfo) -> Option<std::path::PathBuf> {
        match self {
            IoriCache::Memory(cache) => cache.segment_path(segment).await,
            IoriCache::File(cache) => cache.segment_path(segment).await,
        }
    }

    async fn invalidate(&self, segment: &SegmentInfo) -> IoriResult<()> {
        match self {
            IoriCache::Memory(cache) => cache.invalidate(segment).await,
            IoriCache::File(cache) => cache.invalidate(segment).await,
        }
    }

    async fn clear(&self) -> IoriResult<()> {
        match self {
            IoriCache::Memory(cache) => cache.clear().await,
            IoriCache::File(cache) => cache.clear().await,
        }
    }

    fn location_hint(&self) -> Option<String> {
        match self {
            IoriCache::Memory(cache) => cache.location_hint(),
            IoriCache::File(cache) => cache.location_hint(),
        }
    }
}
