use std::{collections::HashMap, path::PathBuf};

use tokio::fs::File;

use super::ConsumerOutput;
use crate::{error::IoriResult, StreamingSegment};

pub struct PipeConsumer {
    output_dir: PathBuf,
}

impl PipeConsumer {
    pub fn new<P>(output_dir: P) -> IoriResult<Self>
    where
        P: Into<PathBuf>,
    {
        Ok(Self {
            output_dir: output_dir.into(),
        })
    }

    pub async fn open_writer(
        &self,
        segment: &impl StreamingSegment,
    ) -> IoriResult<Option<ConsumerOutput>> {
        let path = self.output_dir.join(segment.file_name());
        let file = File::create(path).await?;
        // TODO: map sequence -> sequence
        Ok(Some(Box::pin(file)))
    }
}
