use super::{CacheSource, CacheSourceReader, CacheSourceWriter};
use crate::error::IoriResult;
use std::path::PathBuf;
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

pub use opendal::*;

pub struct OpendalCacheSource {
    operator: Operator,
    prefix: String,

    with_internal_prefix: bool,
}

impl OpendalCacheSource {
    pub fn new(operator: Operator, prefix: impl Into<String>, with_internal_prefix: bool) -> Self {
        Self {
            operator,
            prefix: prefix.into(),
            with_internal_prefix,
        }
    }

    fn segment_key(&self, segment: &crate::SegmentInfo) -> String {
        let prefix = &self.prefix;
        let filename = segment.file_name.replace('/', "__");
        if self.with_internal_prefix {
            let stream_id = segment.stream_id;
            let sequence = segment.sequence;
            format!("{prefix}/{stream_id:02}_{sequence:06}_{filename}")
        } else {
            format!("{prefix}/{filename}")
        }
    }
}

impl CacheSource for OpendalCacheSource {
    async fn open_writer(
        &self,
        segment: &crate::SegmentInfo,
    ) -> IoriResult<Option<CacheSourceWriter>> {
        let key = self.segment_key(segment);

        if self.operator.exists(&key).await? {
            tracing::warn!("File {} already exists, ignoring.", key);
            return Ok(None);
        }

        let writer = self
            .operator
            .writer_with(&key)
            .chunk(5 * 1024 * 1024)
            .await?
            .into_futures_async_write()
            .compat_write();
        Ok(Some(Box::new(writer)))
    }

    async fn open_reader(&self, segment: &crate::SegmentInfo) -> IoriResult<CacheSourceReader> {
        let key = self.segment_key(segment);
        let stat = self.operator.stat(&key).await?;
        let length = stat.content_length();
        let reader = self
            .operator
            .reader(&key)
            .await?
            .into_futures_async_read(0..length)
            .await?
            .compat();

        Ok(Box::new(reader))
    }

    async fn segment_path(&self, segment: &crate::SegmentInfo) -> Option<PathBuf> {
        Some(PathBuf::from(self.segment_key(segment)))
    }

    async fn invalidate(&self, segment: &crate::SegmentInfo) -> IoriResult<()> {
        let key = self.segment_key(segment);
        self.operator.delete(&key).await?;
        Ok(())
    }

    async fn clear(&self) -> IoriResult<()> {
        self.operator.remove_all(&self.prefix).await?;
        Ok(())
    }

    fn location_hint(&self) -> Option<String> {
        Some(self.prefix.clone())
    }
}
