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

    async fn ensure_cache_dir(&self) -> IoriResult<()> {
        if !self.cache_dir.exists() {
            tokio::fs::create_dir_all(&self.cache_dir).await?;
        }

        Ok(())
    }

    fn segment_path(&self, segment: &crate::SegmentInfo) -> PathBuf {
        let filename = segment.file_name.replace('/', "__");
        let stream = segment.stream;
        let sequence = segment.sequence;
        let filename = format!("{stream:02}_{sequence:06}_{filename}");
        self.cache_dir.join(filename)
    }
}

impl CacheSource for FileCacheSource {
    async fn open_writer(
        &self,
        segment: &crate::SegmentInfo,
    ) -> IoriResult<Option<CacheSourceWriter>> {
        self.ensure_cache_dir().await?;

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

    async fn open_reader(&self, segment: &crate::SegmentInfo) -> IoriResult<CacheSourceReader> {
        let path = self.segment_path(segment);
        let file = File::open(path).await?;
        Ok(Box::new(file))
    }

    async fn segment_path(&self, segment: &crate::SegmentInfo) -> Option<PathBuf> {
        Some(self.segment_path(segment))
    }

    async fn invalidate(&self, segment: &crate::SegmentInfo) -> IoriResult<()> {
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
