use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use tokio::sync::{mpsc, RwLock, Semaphore};

use crate::{error::IoriResult, StreamingSegment, StreamingSource};

pub struct ParallelDownloader<S>
where
    S: StreamingSource,
{
    source: Arc<RwLock<S>>,
    concurrency: NonZeroU32,
    permits: Arc<Semaphore>,

    total: Arc<AtomicUsize>,
    downloaded: Arc<AtomicUsize>,
}

impl<S> ParallelDownloader<S>
where
    S: StreamingSource + Send + Sync + 'static,
{
    pub fn new(source: S, concurrency: NonZeroU32) -> Self {
        let permits = Arc::new(Semaphore::new(concurrency.get() as usize));

        Self {
            source: Arc::new(RwLock::new(source)),
            concurrency,
            permits,

            total: Arc::new(AtomicUsize::new(0)),
            downloaded: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn download(&mut self) -> IoriResult<()> {
        log::info!(
            "Start downloading with {} thread(s).",
            self.concurrency.get()
        );

        let mut receiver = self.get_receiver().await?;
        while let Some(segments) = receiver.recv().await {
            self.total.fetch_add(segments.len(), Ordering::Relaxed);
            log::info!("{} new segments were added to queue.", segments.len());

            for segment in segments {
                let permit = self.permits.clone().acquire_owned().await.unwrap();
                let segments_downloaded = self.downloaded.clone();
                let segments_total = self.total.clone();
                let source: Arc<RwLock<S>> = self.source.clone();
                tokio::spawn(async move {
                    let segment = source.read().await.fetch_segment(segment).await.unwrap();
                    // semaphore is only used to limit download concurrency, so drop it directly after fetching
                    drop(permit);

                    let filename = segment.file_name();

                    let downloaded = segments_downloaded.fetch_add(1, Ordering::Relaxed) + 1;
                    let total = segments_total.load(Ordering::Relaxed);
                    let percentage = if total == 0 {
                        0.
                    } else {
                        downloaded as f32 / total as f32 * 100.
                    };
                    // Avg Speed: 1.00 chunks/s or 5.02x | ETA: 6m 37s
                    log::info!(
                        "Processing {filename} finished. ({downloaded} / {total} or {percentage:.2}%)"
                    );
                });
            }
        }

        // wait for all tasks to finish
        let _ = self
            .permits
            .acquire_many(self.concurrency.get() as u32)
            .await
            .unwrap();

        Ok(())
    }

    async fn get_receiver(&mut self) -> IoriResult<mpsc::UnboundedReceiver<Vec<S::Segment>>> {
        self.source.write().await.fetch_info().await
    }
}