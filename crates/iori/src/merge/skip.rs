use std::{marker::PhantomData, path::PathBuf};

use tokio::fs::File;

use crate::{error::IoriResult, merge::concat::segment_path, StreamingSegment, ToSegmentData};

use super::Merger;

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
    S: StreamingSegment + ToSegmentData + Send + Sync + 'static,
{
    type Segment = S;
    type MergeSegment = File;
    type MergeResult = ();

    async fn open_writer(
        &self,
        segment: &Self::Segment,
    ) -> crate::error::IoriResult<Option<Self::MergeSegment>> {
        let path = segment_path(segment, &self.output_dir);
        if path
            .metadata()
            .map(|p| p.is_file() && p.len() > 0)
            .unwrap_or_default()
        {
            log::warn!("File {} already exists, ignoring.", path.display());
            return Ok(None);
        }

        let tmp_file = File::create(path).await?;
        Ok(Some(tmp_file))
    }

    async fn update(&mut self, _segment: Self::Segment) -> IoriResult<()> {
        Ok(())
    }

    async fn fail(&mut self, _segment: Self::Segment) -> IoriResult<()> {
        Ok(())
    }

    async fn finish(&mut self) -> IoriResult<Self::MergeResult> {
        log::info!("Skip merging. Please merge video chunks manually.");
        log::info!(
            "Temporary files are located at {}",
            self.output_dir.display()
        );
        Ok(())
    }
}
