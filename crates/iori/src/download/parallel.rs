use crate::{
    cache::CacheSource, error::IoriResult, merge::Merger, IoriError, SegmentInfo, StreamingSegment,
    StreamingSource,
};
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, Semaphore};

struct ParallelDownloader<S, M, C>
where
    S: StreamingSource,
    M: Merger,
    C: CacheSource,
{
    source: Arc<S>,
    concurrency: NonZeroU32,
    permits: Arc<Semaphore>,

    total: Arc<AtomicUsize>,
    downloaded: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
    failed_segments_name: Arc<Mutex<Vec<String>>>,

    cache: Arc<C>,
    merger: Arc<Mutex<M>>,

    retries: u32,
}

impl<S, M, C> ParallelDownloader<S, M, C>
where
    S: StreamingSource + Send + Sync + 'static,
    M: Merger + Send + Sync + 'static,
    C: CacheSource + Send + Sync + 'static,
{
    pub(crate) fn new(
        source: S,
        merger: M,
        cache: C,
        concurrency: NonZeroU32,
        retries: u32,
    ) -> Self {
        let permits = Arc::new(Semaphore::new(concurrency.get() as usize));

        Self {
            source: Arc::new(source),
            merger: Arc::new(Mutex::new(merger)),
            cache: Arc::new(cache),
            concurrency,
            permits,

            total: Arc::new(AtomicUsize::new(0)),
            downloaded: Arc::new(AtomicUsize::new(0)),
            failed: Arc::new(AtomicUsize::new(0)),
            failed_segments_name: Arc::new(Mutex::new(Vec::new())),

            retries,
        }
    }

    pub async fn download(self) -> IoriResult<M::Result> {
        tracing::info!(
            "Start downloading with {} thread(s).",
            self.concurrency.get()
        );

        let mut receiver = self.source.fetch_info().await?;

        // ctrl-c handler
        let is_closed = Arc::new(AtomicBool::new(false));
        let is_closed_inner = is_closed.clone();
        let ctrlc_handler = tokio::spawn(async move {
            // wait for the first ctrl-c to stop downloader
            tokio::signal::ctrl_c().await.unwrap();
            tracing::info!("Ctrl-C received, stopping downloader.");
            is_closed_inner.store(true, Ordering::Relaxed);

            // wait for the second ctrl-c to force exit
            tokio::signal::ctrl_c().await.unwrap();
            tracing::info!("Ctrl-C received again, force exit.");
            std::process::exit(1);
        });

        while let Some(segments) = receiver.recv().await {
            // If the playlist is not available, the downloader will be stopped.
            if let Err(e) = segments {
                tracing::error!("Failed to fetch segment list: {e}");
                return Err(e);
            }
            let segments = segments?;

            self.total.fetch_add(segments.len(), Ordering::Relaxed);
            tracing::info!("{} new segments were added to queue.", segments.len());

            for segment in segments {
                let segment_info = SegmentInfo::from(&segment);

                let permit = self.permits.clone().acquire_owned().await.unwrap();
                let segments_downloaded = self.downloaded.clone();
                let segments_failed = self.failed.clone();
                let failed_segments_name = self.failed_segments_name.clone();
                let segments_total = self.total.clone();

                let source = self.source.clone();
                let merger = self.merger.clone();
                let cache = self.cache.clone();
                let merge_segment = cache.open_writer(&segment_info).await?;
                let Some(mut writer) = merge_segment else {
                    segments_downloaded.fetch_add(1, Ordering::Relaxed);
                    _ = merger.lock().await.update(segment_info, cache).await;
                    continue;
                };

                let mut retries = self.retries;
                tokio::spawn(async move {
                    let filename = segment.file_name();

                    loop {
                        // Workaround for `higher-ranked lifetime error`
                        let result = assert_send(source.fetch_segment(&segment, &mut writer)).await;
                        let result = match result {
                            // graceful shutdown
                            Ok(_) => writer.shutdown().await.map_err(IoriError::IOError),
                            Err(e) => Err(e),
                        };
                        match result {
                            Ok(_) => break,
                            Err(e) => {
                                if retries == 0 {
                                    tracing::error!(
                                        "Processing {filename} failed, max retries exceed, drop. {e}"
                                    );
                                    failed_segments_name
                                        .lock()
                                        .await
                                        .push(segment.file_name().to_string());
                                    segments_failed.fetch_add(1, Ordering::Relaxed);
                                    _ = merger.lock().await.fail(segment_info, cache).await;
                                    return;
                                }

                                retries -= 1;
                                tracing::warn!("Processing {filename} failed, retry later. {e}");
                            }
                        }
                    }

                    // drop writer to flush and save the data
                    drop(writer);

                    // here we can not drop semaphore, because the merger might take some time to process the merging

                    let downloaded = segments_downloaded.fetch_add(1, Ordering::Relaxed)
                        + 1
                        + segments_failed.load(Ordering::Relaxed);
                    let total = segments_total.load(Ordering::Relaxed);
                    let percentage = if total == 0 {
                        0.
                    } else {
                        downloaded as f32 / total as f32 * 100.
                    };
                    // Avg Speed: 1.00 chunks/s or 5.02x | ETA: 6m 37s
                    tracing::info!(
                        "Processing {filename} finished. ({downloaded} / {total} or {percentage:.2}%)"
                    );

                    _ = merger.lock().await.update(segment_info, cache).await;

                    // drop permit to release the semaphore
                    drop(permit);
                });
            }

            if is_closed.load(Ordering::Relaxed) {
                break;
            }
        }

        // drop receiver to stop the source from fetching more segments
        drop(receiver);

        // wait for all tasks to finish
        let _ = self
            .permits
            .acquire_many(self.concurrency.get())
            .await
            .unwrap();

        let failed = self.failed_segments_name.lock().await;
        if !failed.is_empty() {
            tracing::error!("Failed to download {} segments:", failed.len());
            for segment in failed.iter() {
                tracing::error!("  - {}", segment);
            }
        }

        ctrlc_handler.abort();
        self.merger.lock().await.finish(self.cache).await
    }
}

// https://github.com/rust-lang/rust/issues/102211#issuecomment-1371414544
// TODO: remove this when this issue is fixed
fn assert_send<'a, T>(
    fut: impl std::future::Future<Output = T> + Send + 'a,
) -> impl std::future::Future<Output = T> + Send + 'a {
    fut
}

pub struct ParallelDownloaderBuilder<M, C, MR = ()> {
    concurrency: NonZeroU32,
    retries: u32,
    merger: Option<M>,
    cache: Option<C>,

    _merge_result: std::marker::PhantomData<MR>,
}

impl<M, C, MR> ParallelDownloaderBuilder<M, C, MR>
where
    M: Merger<Result = MR> + Send + Sync + 'static,
    C: CacheSource,
{
    pub fn new() -> Self {
        Self {
            concurrency: NonZeroU32::new(5).unwrap(),
            retries: 3,
            merger: None,
            cache: None,
            _merge_result: Default::default(),
        }
    }

    pub fn concurrency(mut self, concurrency: NonZeroU32) -> Self {
        self.concurrency = concurrency;
        self
    }

    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    pub fn merger(mut self, merger: M) -> Self {
        self.merger = Some(merger);
        self
    }

    pub fn cache(mut self, cache: C) -> Self {
        self.cache = Some(cache);
        self
    }

    fn build<S>(self, source: S) -> ParallelDownloader<S, M, C>
    where
        S: StreamingSource + Send + Sync + 'static,
    {
        ParallelDownloader::new(
            source,
            self.merger.expect("Merger is not set"),
            self.cache.expect("Cache is not set"),
            self.concurrency,
            self.retries,
        )
    }

    pub async fn download<S>(self, source: S) -> IoriResult<MR>
    where
        S: StreamingSource + Send + Sync + 'static,
    {
        let downloader = self.build(source);
        downloader.download().await
    }
}

impl<M, C, MR> Default for ParallelDownloaderBuilder<M, C, MR>
where
    M: Merger<Result = MR> + Send + Sync + 'static,
    C: CacheSource,
{
    fn default() -> Self {
        Self::new()
    }
}
