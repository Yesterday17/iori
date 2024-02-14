use std::{num::NonZeroUsize, sync::Arc};

use tokio::sync::{Mutex, Semaphore};

use crate::StreamingSource;

pub struct SequencialDownloader<S>
where
    S: StreamingSource,
{
    source: S,
}

impl<S> SequencialDownloader<S>
where
    S: StreamingSource,
{
    pub fn new(source: S) -> Self {
        Self { source }
    }

    pub async fn download(&mut self) {
        let mut receiver = self.source.fetch_info().await;
        while let Some(segment) = receiver.recv().await {
            self.source.fetch_segment(segment).await;
        }
    }
}

pub struct ParallelDownloader<S>
where
    S: StreamingSource,
{
    source: Arc<Mutex<S>>,
    permits: Arc<Semaphore>,
}

impl<S> ParallelDownloader<S>
where
    S: StreamingSource + Send + Sync + 'static,
{
    pub fn new(source: S, concurrency: NonZeroUsize) -> Self {
        let permits = Arc::new(Semaphore::new(concurrency.get()));

        Self {
            source: Arc::new(Mutex::new(source)),
            permits,
        }
    }

    pub async fn download(&mut self) {
        let mut receiver = self.source.lock().await.fetch_info().await;
        while let Some(segment) = receiver.recv().await {
            let permit = self.permits.clone().acquire_owned().await.unwrap();
            let source = self.source.clone();
            tokio::spawn(async move {
                source.lock().await.fetch_segment(segment).await;
                drop(permit);
            });
        }
    }
}
