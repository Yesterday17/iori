use super::{BoxedStreamingSegment, Merger};
use crate::{cache::CacheSource, error::IoriResult, StreamingSegment};
use std::path::{Path, PathBuf};
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
        _cache: impl CacheSource,
    ) -> IoriResult<()> {
        self.segments.push(ConcatSegment {
            segment: Box::new(segment),
            success: true,
        });
        Ok(())
    }

    async fn fail(
        &mut self,
        segment: impl StreamingSegment + Send + Sync + 'static,
        cache: impl CacheSource,
    ) -> IoriResult<()> {
        cache.invalidate(&segment).await?;
        self.segments.push(ConcatSegment {
            segment: Box::new(segment),
            success: false,
        });
        Ok(())
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
        log::info!("Merging chunks...");
        concat_merge(&mut self.segments, &cache, &self.output_file).await?;

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

fn trim_end<T>(input: &[T], should_skip: fn(&T) -> bool) -> &[T] {
    let mut end = input.len();
    while end > 0 && should_skip(&input[end - 1]) {
        end -= 1;
    }
    &input[..end]
}

pub(crate) struct ConcatMergeNamer {
    file_count: u32,
    file_extension: String,
}

impl ConcatMergeNamer {
    pub fn new<P>(output_path: P) -> Self
    where
        P: AsRef<Path>,
    {
        let file_extension = output_path
            .as_ref()
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_string();

        Self {
            file_count: 0,
            file_extension,
        }
    }

    pub fn next(&mut self) -> PathBuf {
        self.file_count += 1;
        PathBuf::from(format!("{}.{}", self.file_count, self.file_extension))
    }
}

pub(crate) struct ConcatSegment<S> {
    pub segment: S,
    pub success: bool,
}

async fn concat_merge<S, O>(
    segments: &mut Vec<ConcatSegment<S>>,
    cache: &impl CacheSource,
    output_path: O,
) -> IoriResult<()>
where
    S: StreamingSegment,
    O: AsRef<Path>,
{
    segments.sort_by(|a, b| a.segment.sequence().cmp(&b.segment.sequence()));
    let segments = trim_end(&segments, |s| !s.success);

    let mut namer = ConcatMergeNamer::new(&output_path);
    let mut output = File::create(output_path.as_ref()).await?;
    for segment in segments {
        let success = segment.success;
        let segment = &segment.segment;
        if !success {
            output = File::create(namer.next()).await?;
        }

        let mut reader = cache.open_reader(segment).await?;
        tokio::io::copy(&mut reader, &mut output).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_trim_end() {
        let input = [1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0];
        let output = super::trim_end(&input, |&x| x == 0);
        assert_eq!(output, [1, 2, 3]);

        let input = [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3];
        let output = super::trim_end(&input, |&x| x == 0);
        assert_eq!(output, input);

        let input = [1, 2, 3, 0, 0, 3, 0, 0, 0];
        let output = super::trim_end(&input, |&x| x == 0);
        assert_eq!(output, [1, 2, 3, 0, 0, 3]);
    }
}
