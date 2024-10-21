use super::Merger;
use crate::{cache::CacheSource, error::IoriResult, StreamingSegment};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::{Mutex, MutexGuard};

pub struct PipeMerger<S> {
    recycle: bool,

    next: Arc<AtomicU64>,
    segments: Arc<Mutex<HashMap<u64, Option<S>>>>,
}

impl<S> PipeMerger<S>
where
    S: StreamingSegment + Send + 'static,
{
    pub fn new(recycle: bool) -> Self {
        Self {
            recycle,

            next: Arc::new(AtomicU64::new(0)),
            segments: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn pipe_segments(
        &self,
        mut segments: MutexGuard<'_, HashMap<u64, Option<S>>>,
        cache: &impl CacheSource,
    ) -> IoriResult<()> {
        while let Some(segment) = segments.remove(&self.next.load(Ordering::Relaxed)) {
            if let Some(segment) = segment {
                let mut reader = cache.open_reader(&segment).await?;
                _ = tokio::io::copy(&mut reader, &mut tokio::io::stdout()).await;
                if self.recycle {
                    cache.invalidate(&segment).await?;
                }
            }

            self.next.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
}

impl<S> Merger for PipeMerger<S>
where
    S: StreamingSegment + Send + 'static,
{
    type Segment = S;
    type Result = ();

    async fn update(&mut self, segment: Self::Segment, cache: &impl CacheSource) -> IoriResult<()> {
        // Hold the lock so that no one would be able to write new segments and modify `next`
        let mut segments = self.segments.lock().await;
        let sequence = segment.sequence();

        // write file path to HashMap
        segments.insert(sequence, Some(segment));

        if sequence == self.next.load(Ordering::Relaxed) {
            self.pipe_segments(segments, cache).await?;
        }

        Ok(())
    }

    async fn fail(&mut self, segment: Self::Segment, cache: &impl CacheSource) -> IoriResult<()> {
        cache.invalidate(&segment).await?;

        // Hold the lock so that no one would be able to write new segments and modify `next`
        let mut segments = self.segments.lock().await;
        let sequence = segment.sequence();

        // ignore the result
        segments.insert(sequence, None);
        self.pipe_segments(segments, cache).await?;

        Ok(())
    }

    async fn finish(&mut self, cache: &impl CacheSource) -> IoriResult<Self::Result> {
        if self.recycle {
            cache.clear().await?;
        }

        Ok(())
    }
}
