use super::{CacheSource, CacheSourceReader, CacheSourceWriter};
use crate::error::IoriResult;
use std::path::PathBuf;
use tokio::fs::File;

pub struct FileCacheSource {
    cache_dir: PathBuf,
}

impl FileCacheSource {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    fn segment_path(&self, segment: &impl crate::StreamingSegment) -> PathBuf {
        let filename = segment.file_name().replace('/', "__");
        let sequence = segment.sequence();
        let filename = format!("{sequence:06}_{filename}");
        self.cache_dir.join(filename)
    }
}

impl CacheSource for FileCacheSource {
    async fn open_writer(
        &self,
        segment: &impl crate::StreamingSegment,
    ) -> IoriResult<Option<CacheSourceWriter>> {
        let path = self.segment_path(segment);
        if path
            .metadata()
            .map(|p| p.is_file() && p.len() > 0)
            .unwrap_or_default()
        {
            log::warn!("File {} already exists, ignoring.", path.display());
            return Ok(None);
        }

        let tmp_file: File = File::create(path).await?;
        Ok(Some(Box::new(tmp_file)))
    }

    async fn open_reader(
        &self,
        segment: &impl crate::StreamingSegment,
    ) -> IoriResult<CacheSourceReader> {
        let path = self.segment_path(segment);
        let file = File::open(path).await?;
        Ok(Box::new(file))
    }

    async fn segment_path(&self, segment: &impl crate::StreamingSegment) -> Option<PathBuf> {
        Some(self.segment_path(segment))
    }

    async fn invalidate(&self, segment: &impl crate::StreamingSegment) -> IoriResult<()> {
        let path = self.segment_path(segment);
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn clear(&self) -> IoriResult<()> {
        tokio::fs::remove_dir_all(&self.cache_dir).await?;
        Ok(())
    }

    fn location_hint(&self) -> Option<String> {
        Some(self.cache_dir.display().to_string())
    }
}
