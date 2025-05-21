use super::Merger;
use crate::{
    cache::CacheSource, error::IoriResult, util::path::DuplicateOutputFileNamer, SegmentInfo,
};
use std::path::PathBuf;
use tokio::fs::File;

/// Concat all segments into a single file after all segments are downloaded.
pub struct ConcatAfterMerger {
    segments: Vec<ConcatSegment>,

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

    async fn update(&mut self, segment: SegmentInfo, _cache: impl CacheSource) -> IoriResult<()> {
        self.segments.push(ConcatSegment {
            segment,
            success: true,
        });
        Ok(())
    }

    async fn fail(&mut self, segment: SegmentInfo, cache: impl CacheSource) -> IoriResult<()> {
        cache.invalidate(&segment).await?;
        self.segments.push(ConcatSegment {
            segment,
            success: false,
        });
        Ok(())
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
        tracing::info!("Merging chunks...");
        concat_merge(&mut self.segments, &cache, self.output_file.clone()).await?;

        if !self.keep_segments {
            tracing::info!("End of merging.");
            tracing::info!("Starting cleaning temporary files.");
            cache.clear().await?;
        }

        tracing::info!(
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

pub(crate) struct ConcatSegment {
    pub segment: SegmentInfo,
    pub success: bool,
}

async fn concat_merge(
    segments: &mut Vec<ConcatSegment>,
    cache: &impl CacheSource,
    output_path: PathBuf,
) -> IoriResult<()> {
    segments.sort_by(|a, b| a.segment.sequence.cmp(&b.segment.sequence));
    let segments = trim_end(&segments, |s| !s.success);

    let mut namer = DuplicateOutputFileNamer::new(output_path.clone());
    let mut output = File::create(output_path).await?;
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
