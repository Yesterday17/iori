use std::{
    future::Future,
    ops::{Deref, DerefMut},
    path::PathBuf,
    pin::Pin,
};
use tokio::io::AsyncWrite;

mod file;
pub use file::FileConsumer;
mod pipe;
pub use pipe::PipeConsumer;

use crate::{error::IoriResult, StreamingSegment};

pub enum Consumer {
    File(FileConsumer),
    Pipe(PipeConsumer),
}

impl Consumer {
    pub fn file(output_dir: impl Into<PathBuf>) -> IoriResult<Self> {
        Ok(Self::File(FileConsumer::new(output_dir)?))
    }

    pub fn pipe(output_dir: impl Into<PathBuf>, recycle: bool) -> IoriResult<Self> {
        Ok(Self::Pipe(PipeConsumer::new(output_dir, recycle)?))
    }

    pub async fn open_writer(
        &self,
        segment: &(impl StreamingSegment + Send + Sync + 'static),
    ) -> IoriResult<Option<ConsumerOutput>> {
        match self {
            Self::File(consumer) => consumer.open_writer(segment).await,
            Self::Pipe(consumer) => consumer.open_writer(segment).await,
        }
    }
}

type ConsumerOutputStream = Pin<Box<dyn AsyncWrite + Send + Sync + 'static>>;

pub struct ConsumerOutput {
    stream: ConsumerOutputStream,
    on_finish: Option<
        Box<
            dyn FnOnce() -> Pin<Box<dyn Future<Output = IoriResult<()>> + Send + 'static>>
                + Send
                + Sync
                + 'static,
        >,
    >,
}

impl ConsumerOutput {
    pub fn new(stream: ConsumerOutputStream) -> Self {
        Self {
            stream,
            on_finish: None,
        }
    }

    pub fn on_finish<F>(mut self, on_finish: F) -> Self
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = IoriResult<()>> + Send + 'static>>
            + Send
            + Sync
            + 'static,
    {
        self.on_finish = Some(Box::new(on_finish));
        self
    }

    pub async fn finish(self) -> IoriResult<()> {
        drop(self.stream);

        if let Some(on_finish) = self.on_finish {
            on_finish().await?;
        }

        Ok(())
    }
}

impl Deref for ConsumerOutput {
    type Target = ConsumerOutputStream;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl DerefMut for ConsumerOutput {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}
