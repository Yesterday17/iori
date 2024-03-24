use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
};

use tokio::sync::{mpsc, Semaphore};

use crate::{error::IoriResult, StreamingSegment, StreamingSource};

pub struct ParallelDownloader<S>
where
    S: StreamingSource,
{
    source: Arc<S>,
    concurrency: NonZeroU32,
    permits: Arc<Semaphore>,

    total: Arc<AtomicUsize>,
    downloaded: Arc<AtomicUsize>,
    failed: Arc<RwLock<Vec<String>>>,

    retries: u32,
}

impl<S> ParallelDownloader<S>
where
    S: StreamingSource + Send + Sync + 'static,
{
    pub fn new(source: S, concurrency: NonZeroU32, retries: u32) -> Self {
        let permits = Arc::new(Semaphore::new(concurrency.get() as usize));

        Self {
            source: Arc::new(source),
            concurrency,
            permits,

            total: Arc::new(AtomicUsize::new(0)),
            downloaded: Arc::new(AtomicUsize::new(0)),
            failed: Arc::new(RwLock::new(Vec::new())),

            retries,
        }
    }

    pub async fn download(&mut self) -> IoriResult<()> {
        log::info!(
            "Start downloading with {} thread(s).",
            self.concurrency.get()
        );

        let mut receiver = self.get_receiver().await?;
        while let Some(segments) = receiver.recv().await {
            // If the playlist is not available, the downloader will be stopped.
            if let Err(e) = segments {
                log::error!("Failed to fetch segment list: {e}");
                return Err(e);
            }
            let segments = segments?;

            self.total.fetch_add(segments.len(), Ordering::Relaxed);
            log::info!("{} new segments were added to queue.", segments.len());

            for segment in segments {
                let permit = self.permits.clone().acquire_owned().await.unwrap();
                let segments_downloaded = self.downloaded.clone();
                let segments_failed = self.failed.clone();
                let segments_total = self.total.clone();
                let source = self.source.clone();
                let mut retries = self.retries;
                tokio::spawn(async move {
                    let filename = segment.file_name();
                    loop {
                        let result = source.fetch_segment(&segment).await;
                        match result {
                            Ok(_) => break,
                            Err(e) => {
                                if retries == 0 {
                                    log::error!(
                                        "Processing {filename} failed, max retries exceed, drop. {e}"
                                    );
                                    segments_failed
                                        .write()
                                        .unwrap()
                                        .push(segment.file_name().to_string());
                                    return;
                                }

                                retries -= 1;
                                log::warn!("Processing {filename} failed, retry later. {e}")
                            }
                        }
                    }

                    // semaphore is only used to limit download concurrency, so drop it directly after fetching
                    drop(permit);

                    let downloaded = segments_downloaded.fetch_add(1, Ordering::Relaxed)
                        + 1
                        + segments_failed.read().unwrap().len();
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

        let failed = self.failed.read().unwrap();
        if !failed.is_empty() {
            log::error!("Failed to download {} segments:", failed.len());
            for segment in failed.iter() {
                log::error!("  - {}", segment);
            }
        }

        Ok(())
    }

    async fn get_receiver(
        &mut self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<S::Segment>>>> {
        self.source.fetch_info().await
    }
}
