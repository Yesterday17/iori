use super::{CacheSource, CacheSourceReader, CacheSourceWriter};
use crate::error::IoriResult;
use std::{
    collections::HashMap,
    io::{self, Cursor},
    mem,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
};

pub struct MemoryCacheSource {
    cache: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
}

impl MemoryCacheSource {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl CacheSource for MemoryCacheSource {
    async fn open_writer(
        &self,
        segment: &crate::SegmentInfo,
    ) -> IoriResult<Option<CacheSourceWriter>> {
        let key = segment.sequence;
        let cache = self.cache.lock().unwrap();
        if cache.contains_key(&key) {
            log::warn!("File {} already exists, ignoring.", key);
            return Ok(None);
        }

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
            .remove(&segment.sequence)
            .unwrap_or_default();
        let reader = Cursor::new(data);
        Ok(Box::new(reader))
    }

    async fn invalidate(&self, segment: &crate::SegmentInfo) -> IoriResult<()> {
        self.cache.lock().unwrap().remove(&segment.sequence);
        Ok(())
    }

    async fn clear(&self) -> IoriResult<()> {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
        Ok(())
    }
}

struct MemoryWriter {
    key: u64,
    cache: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
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
        self.cache
            .lock()
            .unwrap()
            .insert(self.key, cursor.into_inner());
    }
}

#[cfg(test)]
mod tests {
    use crate::{SegmentFormat, SegmentInfo, StreamingSegment};

    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct TestStreamingSegment {
        sequence: u64,
        file_name: &'static str,
    }

    impl TestStreamingSegment {
        fn new(sequence: u64, file_name: &'static str) -> Self {
            Self {
                sequence,
                file_name,
            }
        }
    }

    impl StreamingSegment for TestStreamingSegment {
        fn stream_id(&self) -> u64 {
            0
        }

        fn sequence(&self) -> u64 {
            self.sequence
        }

        fn file_name(&self) -> &str {
            &self.file_name
        }

        fn key(&self) -> Option<std::sync::Arc<crate::decrypt::IoriKey>> {
            None
        }

        fn r#type(&self) -> crate::SegmentType {
            crate::SegmentType::Video
        }

        fn format(&self) -> SegmentFormat {
            SegmentFormat::Mpeg2TS
        }
    }

    #[tokio::test]
    async fn test_memory_cache() {
        let cache = MemoryCacheSource::new();
        let segment = TestStreamingSegment::new(0, "test.ts");
        let segment_info = SegmentInfo::from(&segment);

        let mut writer = cache.open_writer(&segment_info).await.unwrap().unwrap();
        writer.write_all(b"hello").await.unwrap();
        drop(writer);

        let mut reader = cache.open_reader(&segment_info).await.unwrap();
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await.unwrap();
        assert_eq!(data, b"hello");

        cache.invalidate(&segment_info).await.unwrap();
        let mut reader = cache.open_reader(&segment_info).await.unwrap();
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await.unwrap();
        assert_eq!(data, b"");
    }
}
