use std::path::PathBuf;
use tokio::fs::File;

use super::ConsumerOutput;
use crate::{error::IoriResult, StreamingSegment};

pub struct FileConsumer {
    output_dir: PathBuf,
}

impl FileConsumer {
    pub fn new<P>(output_dir: P) -> IoriResult<Self>
    where
        P: Into<PathBuf>,
    {
        let output_dir = output_dir.into();
        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir)?;
        }
        Ok(Self { output_dir })
    }

    pub async fn open_writer(
        &self,
        segment: &impl StreamingSegment,
    ) -> IoriResult<Option<ConsumerOutput>> {
        let filename = segment.file_name();
        let sequence = segment.sequence();
        let filename = format!("{sequence:06}_{filename}");
        let path = self.output_dir.join(filename);
        if path
            .metadata()
            .map(|p| p.is_file() && p.len() > 0)
            .unwrap_or_default()
        {
            log::warn!("File {} already exists, ignoring.", path.display());
            return Ok(None);
        }
        let tmp_file = File::create(path).await?;
        Ok(Some(Box::pin(tmp_file)))
    }
}
