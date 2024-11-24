pub mod file;
pub mod memory;

use crate::{error::IoriResult, StreamingSegment};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

pub type CacheSourceReader = Box<dyn AsyncRead + Unpin + Send + Sync + 'static>;
pub type CacheSourceWriter = Box<dyn AsyncWrite + Unpin + Send + Sync + 'static>;

/// A cache source for storing the downloaded but not merged segments.
pub trait CacheSource: Sync {
    /// Open a writer for writing data of the segment.
    fn open_writer(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl std::future::Future<Output = IoriResult<Option<CacheSourceWriter>>> + Send;

    /// Open a reader for reading data of the segment.
    fn open_reader(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl std::future::Future<Output = IoriResult<CacheSourceReader>> + Send;

    /// Invalidate the cache of the segment from the cache source.
    fn invalidate(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl std::future::Future<Output = IoriResult<()>> + Send;

    /// Clear the cache source.
    fn clear(&self) -> impl std::future::Future<Output = IoriResult<()>> + Send;

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
        segment: &impl StreamingSegment,
    ) -> impl std::future::Future<Output = IoriResult<Option<CacheSourceWriter>>> + Send {
        self.as_ref().open_writer(segment)
    }

    fn open_reader(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl std::future::Future<Output = IoriResult<CacheSourceReader>> + Send {
        self.as_ref().open_reader(segment)
    }

    fn invalidate(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl std::future::Future<Output = IoriResult<()>> + Send {
        self.as_ref().invalidate(segment)
    }

    fn clear(&self) -> impl std::future::Future<Output = IoriResult<()>> {
        self.as_ref().clear()
    }

    fn location_hint(&self) -> Option<String> {
        self.as_ref().location_hint()
    }
}
