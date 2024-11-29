use std::path::{Path, PathBuf};

use super::{BoxedStreamingSegment, Merger};
use crate::{cache::CacheSource, error::IoriResult, StreamingSegment};
use tokio::fs::File;

/// Concat all segments into a single file after all segments are downloaded.
pub struct ConcatAfterMerger {
    segments: Vec<ConcatSegment<BoxedStreamingSegment<'static>>>,

    /// Final output file path.
    output_file: PathBuf,
    /// Keep downloaded segments after merging.
    keep_segments: bool,
}

impl ConcatAfterMerger {
    pub fn new(output_file: PathBuf, keep_segments: bool) -> Self {
        Self {
            segments: Vec::new(),
            output_file,
            keep_segments,
        }
    }
}

impl Merger for ConcatAfterMerger {
    type Result = ();

    async fn update(
        &mut self,
        segment: impl StreamingSegment + Send + Sync + 'static,
        _cache: &impl CacheSource,
    ) -> IoriResult<()> {
        self.segments.push(ConcatSegment(Box::new(segment), true));
        Ok(())
    }

    async fn fail(
        &mut self,
        segment: impl StreamingSegment + Send + Sync + 'static,
        cache: &impl CacheSource,
    ) -> IoriResult<()> {
        cache.invalidate(&segment).await?;
        self.segments.push(ConcatSegment(Box::new(segment), false));
        Ok(())
    }

    async fn finish(&mut self, cache: &impl CacheSource) -> IoriResult<Self::Result> {
        log::info!("Merging chunks...");
        concat_merge(&mut self.segments, cache, &self.output_file).await?;

        if !self.keep_segments {
            log::info!("End of merging.");
            log::info!("Starting cleaning temporary files.");
            cache.clear().await?;
        }

        log::info!(
            "All finished. Please checkout your files at {}",
            self.output_file.display()
        );
        Ok(())
    }
}

struct ConcatSegment<S>(S, bool /* success */);

async fn concat_merge<S, O>(
    segments: &mut Vec<ConcatSegment<S>>,
    cache: &impl CacheSource,
    output_path: O,
) -> IoriResult<()>
where
    S: StreamingSegment,
    O: AsRef<Path>,
{
    segments.sort_by(|a, b| a.0.sequence().cmp(&b.0.sequence()));

    let mut file_count = 0;
    let file_extension = output_path
        .as_ref()
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default();

    let mut output = File::create(output_path.as_ref()).await?;
    for segment in segments {
        let success = segment.1;
        let segment = &segment.0;
        if !success {
            // FIXME: may create an empty file if it is the last segment
            file_count += 1;
            output = File::create(
                output_path
                    .as_ref()
                    .with_extension(format!("{file_count}.{file_extension}")),
            )
            .await?;
        }

        let mut reader = cache.open_reader(segment).await?;
        tokio::io::copy(&mut reader, &mut output).await?;
    }
    Ok(())
}
