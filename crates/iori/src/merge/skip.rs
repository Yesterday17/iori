use super::Merger;
use crate::{error::IoriResult, StreamingSegment};
use std::{marker::PhantomData, path::PathBuf};

pub struct SkipMerger<S> {
    output_dir: PathBuf,
    _phantom: PhantomData<S>,
}

impl<S> SkipMerger<S> {
    pub fn new<P>(output_dir: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self {
            output_dir: output_dir.into(),
            _phantom: PhantomData,
        }
    }
}

impl<S> Merger for SkipMerger<S>
where
    S: StreamingSegment + Send + 'static,
{
    type Segment = S;
    type Result = ();

    async fn update(&mut self, _segment: Self::Segment) -> IoriResult<()> {
        Ok(())
    }

    async fn fail(&mut self, _segment: Self::Segment) -> IoriResult<()> {
        Ok(())
    }

    async fn finish(&mut self) -> IoriResult<Self::Result> {
        log::info!("Skip merging. Please merge video chunks manually.");
        log::info!(
            "Temporary files are located at {}",
            self.output_dir.display()
        );
        Ok(())
    }
}
