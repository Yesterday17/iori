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
    sync::{mpsc, Mutex, MutexGuard},
};

use crate::{error::IoriResult, StreamingSegment, ToSegmentData};

use super::{
    utils::{open_writer, segment_path},
    Merger,
};

pub struct PipeMerger<S> {
    output_dir: PathBuf,
    recycle: bool,

    sender: mpsc::UnboundedSender<S>,
    next: Arc<AtomicU64>,
    segments: Arc<Mutex<HashMap<u64, Option<S>>>>,
}

impl<S> PipeMerger<S>
where
    S: StreamingSegment + Send + Sync + 'static,
{
    pub fn new<P>(output_dir: P, recycle: bool) -> Self
    where
        P: Into<PathBuf>,
    {
        let output_dir = output_dir.into();

        let (sender, mut receiver) = mpsc::unbounded_channel::<S>();
        let output_dir_receiver = output_dir.clone();
        tokio::spawn(async move {
            let stdout = &mut tokio::io::stdout();
            while let Some(segment) = receiver.recv().await {
                let path = segment_path(&segment, &output_dir_receiver);
                let mut file = File::open(&path).await.unwrap();
                tokio::io::copy(&mut file, stdout).await.unwrap();

                if recycle {
                    tokio::fs::remove_file(&path).await.unwrap();
                }
            }
        });

        Self {
            output_dir,
            recycle,

            sender,
            next: Arc::new(AtomicU64::new(0)),
            segments: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn pipe_segments(
        &self,
        mut segments: MutexGuard<'_, HashMap<u64, Option<S>>>,
    ) -> IoriResult<()> {
        while let Some(segment) = segments.remove(&self.next.load(Ordering::Relaxed)) {
            if let Some(segment) = segment {
                let Ok(_) = self.sender.send(segment) else {
                    break;
                };
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
    type Sink = File;
    type Result = ();

    async fn open_writer(&self, segment: &Self::Segment) -> IoriResult<Option<Self::Sink>> {
        open_writer(segment, &self.output_dir).await
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

    async fn finish(&mut self) -> IoriResult<Self::Result> {
        if self.recycle {
            tokio::fs::remove_dir_all(&self.output_dir).await?;
        }

        Ok(())
    }
}
