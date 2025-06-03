use super::{CacheSource, CacheSourceReader, CacheSourceWriter};
use crate::{error::IoriResult, IoriError};
use std::{
    collections::HashMap,
    io::{self, Cursor},
    mem,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
};

#[derive(Default)]
pub struct MemoryCacheSource {
    cache: Arc<Mutex<HashMap<(u64, u64), MemoryEntry>>>,
}

impl MemoryCacheSource {
    pub fn new() -> Self {
        Self::default()
    }

    #[doc(hidden)]
    pub fn into_inner(self: Arc<Self>) -> Arc<Mutex<HashMap<(u64, u64), MemoryEntry>>> {
        self.cache.clone()
    }
}

impl CacheSource for MemoryCacheSource {
    async fn open_writer(
        &self,
        segment: &crate::SegmentInfo,
    ) -> IoriResult<Option<CacheSourceWriter>> {
        let key = (segment.sequence, segment.stream_id);
        let mut cache = self.cache.lock().unwrap();
        if cache.contains_key(&key) {
            tracing::warn!("Cache for {:?} already exists, ignoring.", key);
            return Ok(None);
        }
        cache.insert(key, MemoryEntry::Pending);

        let writer = MemoryWriter {
            key,
            cache: self.cache.clone(),
            inner: Cursor::new(Vec::new()),
        };
        Ok(Some(Box::new(writer)))
    }

    async fn open_reader(&self, segment: &crate::SegmentInfo) -> IoriResult<CacheSourceReader> {
        let data = self
            .cache
            .lock()
            .unwrap()
            .remove(&(segment.sequence, segment.stream_id))
            .unwrap_or_default();
        match data {
            MemoryEntry::Pending => Err(IoriError::IOError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Cache for {:?} not found",
                    (segment.sequence, segment.stream_id)
                ),
            ))),
            MemoryEntry::Data(data) => {
                let reader = Cursor::new(data);
                Ok(Box::new(reader))
            }
        }
    }

    async fn invalidate(&self, segment: &crate::SegmentInfo) -> IoriResult<()> {
        self.cache
            .lock()
            .unwrap()
            .remove(&(segment.sequence, segment.stream_id));
        Ok(())
    }

    async fn clear(&self) -> IoriResult<()> {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
        Ok(())
    }
}

#[derive(Debug, Default)]
pub enum MemoryEntry {
    #[default]
    Pending,
    Data(Vec<u8>),
}

struct MemoryWriter {
    key: (u64, u64),
    cache: Arc<Mutex<HashMap<(u64, u64), MemoryEntry>>>,
    inner: Cursor<Vec<u8>>, // data: Vec<u8>,
}

impl tokio::io::AsyncWrite for MemoryWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        Poll::Ready(io::Write::write(&mut this.inner, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Poll::Ready(io::Write::flush(&mut this.inner))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        self.poll_flush(cx)
    }
}

impl Drop for MemoryWriter {
    fn drop(&mut self) {
        let cursor = mem::take(&mut self.inner);
        self.cache.lock().unwrap().entry(self.key).and_modify(|e| {
            if matches!(e, MemoryEntry::Pending) {
                *e = MemoryEntry::Data(cursor.into_inner());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::{raw::RawSegment, SegmentInfo};

    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_memory_cache() -> IoriResult<()> {
        let cache = MemoryCacheSource::new();
        let segment: RawSegment = RawSegment::new("".to_string(), "ts".to_string());
        let segment_info = SegmentInfo::from(&segment);

        let mut writer = cache.open_writer(&segment_info).await?.unwrap();
        writer.write_all(b"hello").await?;
        writer.shutdown().await?;
        drop(writer);

        let mut reader = cache.open_reader(&segment_info).await?;
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await?;
        assert_eq!(data, b"hello");

        cache.invalidate(&segment_info).await?;
        let result = cache.open_reader(&segment_info).await;
        assert!(result.is_err());

        Ok(())
    }
}
