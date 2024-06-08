use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, RwLock,
    },
};

use tokio::sync::Semaphore;

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

    pub async fn download(&mut self) -> IoriResult<Vec<S::SegmentInfo>> {
        log::info!(
            "Start downloading with {} thread(s).",
            self.concurrency.get()
        );

        let mut receiver = self.source.fetch_info().await?;
        let mut segments_info = Vec::new();

        // ctrl-c handler
        let is_closed = Arc::new(AtomicBool::new(false));
        let is_closed_inner = is_closed.clone();
        let ctrlc_handler = tokio::spawn(async move {
            // wait for the first ctrl-c to stop downloader
            tokio::signal::ctrl_c().await.unwrap();
            log::info!("Ctrl-C received, stopping downloader.");
            is_closed_inner.store(true, Ordering::Relaxed);

            // wait for the second ctrl-c to force exit
            tokio::signal::ctrl_c().await.unwrap();
            log::info!("Ctrl-C received again, force exit.");
            std::process::exit(1);
        });

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
                if let Some(segment_info) = self.source.fetch_segment_info(&segment).await {
                    segments_info.push(segment_info);
                }

                let permit = self.permits.clone().acquire_owned().await.unwrap();
                let segments_downloaded = self.downloaded.clone();
                let segments_failed = self.failed.clone();
                let segments_total = self.total.clone();
                let source = self.source.clone();
                let mut retries = self.retries;
                tokio::spawn(async move {
                    let filename = segment.file_name();
                    loop {
                        let result = source.fetch_segment(&segment, retries > 0).await;
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

            if is_closed.load(Ordering::Relaxed) {
                break;
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

        ctrlc_handler.abort();
        Ok(segments_info)
    }
}
