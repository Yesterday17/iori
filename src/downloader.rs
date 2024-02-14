use std::{num::NonZeroU32, sync::Arc};

use tokio::sync::{RwLock, Semaphore};

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
    source: Arc<RwLock<S>>,
    concurrency: NonZeroU32,
    permits: Arc<Semaphore>,
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
        }
    }

    pub async fn download(&mut self) {
        let mut receiver = self.get_receiver().await;
        while let Some(segment) = receiver.recv().await {
            let permit = self.permits.clone().acquire_owned().await.unwrap();
            let source = self.source.clone();
            tokio::spawn(async move {
                source.read().await.fetch_segment(segment).await;
                drop(permit);
            });
        }

        // wait for all tasks to finish
        let _ = self
            .permits
            .acquire_many(self.concurrency.get() as u32)
            .await
            .unwrap();
    }

    async fn get_receiver(&mut self) -> tokio::sync::mpsc::UnboundedReceiver<S::Segment> {
        self.source.write().await.fetch_info().await
    }
}
