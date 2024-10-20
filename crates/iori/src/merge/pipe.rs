use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use tokio::{
    fs::File,
    sync::{Mutex, MutexGuard},
};

use crate::{error::IoriResult, StreamingSegment, ToSegmentData};

use super::{concat::segment_path, Merger};

pub struct PipeMerger<S> {
    output_dir: PathBuf,
    recycle: bool,

    next: Arc<AtomicU64>,
    segments: Arc<Mutex<HashMap<u64, Option<S>>>>,
}

impl<S> PipeMerger<S> {
    pub fn new<P>(output_dir: P, recycle: bool) -> IoriResult<Self>
    where
        P: Into<PathBuf>,
    {
        let output_dir = output_dir.into();
        std::fs::create_dir_all(&output_dir)?;

        Ok(Self {
            output_dir,
            recycle,

            next: Arc::new(AtomicU64::new(0)),
            segments: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

impl<S> PipeMerger<S>
where
    S: StreamingSegment + ToSegmentData + Send + Sync + 'static,
{
    async fn pipe_segments(
        &self,
        mut segments: MutexGuard<'_, HashMap<u64, Option<S>>>,
    ) -> IoriResult<()> {
        while let Some(segment) = segments.remove(&self.next.load(Ordering::Relaxed)) {
            if let Some(segment) = segment {
                let path = segment_path(&segment, &self.output_dir);
                // open file and write binary content to stdout
                let mut file = std::fs::File::open(&path)?;
                let _ = std::io::copy(&mut file, &mut std::io::stdout());
                if self.recycle {
                    tokio::fs::remove_file(&path).await?;
                }
            }

            self.next.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
}

impl<S> Merger for PipeMerger<S>
where
    S: StreamingSegment + ToSegmentData + Send + Sync + 'static,
{
    type Segment = S;
    type MergeSegment = File;
    type MergeResult = ();

    async fn open_writer(
        &self,
        segment: &Self::Segment,
    ) -> crate::error::IoriResult<Option<Self::MergeSegment>> {
        let path = segment_path(segment, &self.output_dir);
        if path
            .metadata()
            .map(|p| p.is_file() && p.len() > 0)
            .unwrap_or_default()
        {
            log::warn!("File {} already exists, ignoring.", path.display());
            return Ok(None);
        }

        let tmp_file = File::create(path).await?;
        Ok(Some(tmp_file))
    }

    async fn update(&mut self, segment: Self::Segment) -> IoriResult<()> {
        // Hold the lock so that no one would be able to write new segments and modify `next`
        let mut segments = self.segments.lock().await;
        let sequence = segment.sequence();

        // write file path to HashMap
        segments.insert(sequence, Some(segment));

        if sequence == self.next.load(Ordering::Relaxed) {
            self.pipe_segments(segments).await?;
        }

        Ok(())
    }

    async fn fail(&mut self, segment: Self::Segment) -> IoriResult<()> {
        // Hold the lock so that no one would be able to write new segments and modify `next`
        let mut segments = self.segments.lock().await;
        let sequence = segment.sequence();

        // try to remove segment
        let path = segment_path(&segment, &self.output_dir);
        let _ = tokio::fs::remove_file(&path).await;

        // ignore the result
        segments.insert(sequence, None);
        self.pipe_segments(segments).await?;

        Ok(())
    }

    async fn finish(&mut self) -> IoriResult<Self::MergeResult> {
        Ok(())
    }
}
