pub mod file;
pub mod memory;

use crate::{error::IoriResult, StreamingSegment};
use std::{future::Future, path::PathBuf, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};

pub type CacheSourceReader = Box<dyn AsyncRead + Unpin + Send + Sync + 'static>;
pub type CacheSourceWriter = Box<dyn AsyncWrite + Unpin + Send + Sync + 'static>;

/// A cache source for storing the downloaded but not merged segments.
pub trait CacheSource: Send + Sync + 'static {
    /// Open a writer for writing data of the segment.
    fn open_writer(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl Future<Output = IoriResult<Option<CacheSourceWriter>>> + Send;

    /// Open a reader for reading data of the segment.
    fn open_reader(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl Future<Output = IoriResult<CacheSourceReader>> + Send;

    /// Get stored location of the segment
    fn segment_path(
        &self,
        _segment: &impl StreamingSegment,
    ) -> impl Future<Output = Option<PathBuf>> + Send {
        async { None }
    }

    /// Invalidate the cache of the segment from the cache source.
    fn invalidate(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl Future<Output = IoriResult<()>> + Send;

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
        segment: &impl StreamingSegment,
    ) -> impl Future<Output = IoriResult<Option<CacheSourceWriter>>> + Send {
        self.as_ref().open_writer(segment)
    }

    fn open_reader(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl Future<Output = IoriResult<CacheSourceReader>> + Send {
        self.as_ref().open_reader(segment)
    }

    fn segment_path(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl Future<Output = Option<PathBuf>> + Send {
        self.as_ref().segment_path(segment)
    }

    fn invalidate(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl Future<Output = IoriResult<()>> + Send {
        self.as_ref().invalidate(segment)
    }

    fn clear(&self) -> impl Future<Output = IoriResult<()>> {
        self.as_ref().clear()
    }

    fn location_hint(&self) -> Option<String> {
        self.as_ref().location_hint()
    }
}
