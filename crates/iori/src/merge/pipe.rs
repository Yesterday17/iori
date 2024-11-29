use super::Merger;
use crate::{
    cache::CacheSource, error::IoriResult, util::ordered_stream::OrderedStream, StreamingSegment,
};
use std::pin::Pin;
use tokio::{io::AsyncRead, sync::mpsc, task::JoinHandle};

pub struct PipeMerger {
    recycle: bool,

    sender: mpsc::UnboundedSender<(u64, Option<Pin<Box<dyn AsyncRead + Send + Send + 'static>>>)>,
    future: Option<JoinHandle<()>>,
}

impl PipeMerger {
    pub fn new(recycle: bool) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut stream = OrderedStream::new(rx);
        let future = tokio::spawn(async move {
            while let Some(segment) = stream.next().await {
                if let Some(mut reader) = segment {
                    _ = tokio::io::copy(&mut reader, &mut tokio::io::stdout()).await;
                }
            }
        });

        Self {
            recycle,

            sender: tx,
            future: Some(future),
        }
    }
}

impl Merger for PipeMerger {
    type Result = ();

    async fn update(
        &mut self,
        segment: impl StreamingSegment + Send + Sync + 'static,
        cache: &impl CacheSource,
    ) -> IoriResult<()> {
        let reader = cache.open_reader(&segment).await?;
        self.sender
            .send((segment.sequence(), Some(Box::pin(reader))))
            .expect("Failed to send segment");

        Ok(())
    }

    async fn fail(
        &mut self,
        segment: impl StreamingSegment + Send + Sync + 'static,
        cache: &impl CacheSource,
    ) -> IoriResult<()> {
        cache.invalidate(&segment).await?;

        self.sender
            .send((segment.sequence(), None))
            .expect("Failed to send segment");

        Ok(())
    }

    async fn finish(&mut self, cache: &impl CacheSource) -> IoriResult<Self::Result> {
        self.future
            .take()
            .unwrap()
            .await
            .expect("Failed to join pipe");
        if self.recycle {
            cache.clear().await?;
        }

        Ok(())
    }
}
