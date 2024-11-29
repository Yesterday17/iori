use super::Merger;
use crate::{
    cache::CacheSource, error::IoriResult, util::ordered_stream::OrderedStream, StreamingSegment,
};
use std::{future::Future, pin::Pin};
use tokio::{io::AsyncRead, sync::mpsc, task::JoinHandle};

type SendSegment = (
    Pin<Box<dyn AsyncRead + Send + 'static>>,
    Pin<Box<dyn Future<Output = IoriResult<()>> + Send>>,
);

pub struct PipeMerger {
    recycle: bool,

    sender: mpsc::UnboundedSender<(u64, Option<SendSegment>)>,
    future: Option<JoinHandle<()>>,
}

impl PipeMerger {
    pub fn new(recycle: bool) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut stream: OrderedStream<Option<SendSegment>> = OrderedStream::new(rx);
        let mut stdout = tokio::io::stdout();
        let future = tokio::spawn(async move {
            while let Some(segment) = stream.next().await {
                if let Some((mut reader, invalidate)) = segment {
                    _ = tokio::io::copy(&mut reader, &mut stdout).await;
                    if recycle {
                        _ = invalidate.await;
                    }
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
        cache: impl CacheSource,
    ) -> IoriResult<()> {
        let sequence = segment.sequence();
        let reader = cache.open_reader(&segment).await?;
        let invalidate = async move { cache.invalidate(&segment).await };

        self.sender
            .send((sequence, Some((Box::pin(reader), Box::pin(invalidate)))))
            .expect("Failed to send segment");

        Ok(())
    }

    async fn fail(
        &mut self,
        segment: impl StreamingSegment + Send + Sync + 'static,
        cache: impl CacheSource,
    ) -> IoriResult<()> {
        cache.invalidate(&segment).await?;

        self.sender
            .send((segment.sequence(), None))
            .expect("Failed to send segment");

        Ok(())
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
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
