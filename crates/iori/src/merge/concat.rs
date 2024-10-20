use std::path::{Path, PathBuf};

use super::Merger;
use crate::{error::IoriResult, StreamingSegment, ToSegmentData};
use tokio::fs::File;

/// Concat all segments into a single file after all segments are downloaded.
pub struct ConcatAfterMerger<S> {
    segments: Vec<ConcatSegment<S>>,

    /// Temporary directory to store downloaded segments.
    temp_dir: PathBuf,
    /// Final output file path.
    output_file: PathBuf,
}

impl<S> ConcatAfterMerger<S> {
    pub fn new<T>(temp_dir: T, output_file: PathBuf) -> Self
    where
        T: Into<PathBuf>,
    {
        Self {
            segments: Vec::new(),
            temp_dir: temp_dir.into(),
            output_file,
        }
    }
}

impl<S> Merger for ConcatAfterMerger<S>
where
    S: StreamingSegment + ToSegmentData + Send + Sync + 'static,
{
    type Segment = S;
    type MergeSegment = File;
    type MergeResult = ();

    async fn open_writer(&self, segment: &Self::Segment) -> IoriResult<Option<Self::MergeSegment>> {
        let path = self.segment_path(segment);
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

    async fn update(&mut self, segment: Self::Segment) -> IoriResult<()> {
        self.segments.push(ConcatSegment(segment, true));
        Ok(())
    }

    async fn fail(&mut self, segment: Self::Segment) -> IoriResult<()> {
        let path = self.segment_path(&segment);
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }

        self.segments.push(ConcatSegment(segment, false));
        Ok(())
    }

    async fn finish(&mut self) -> IoriResult<Self::MergeResult> {
        log::info!("Merging chunks...");
        concat_merge(&mut self.segments, &self.temp_dir, &self.output_file).await?;

        log::info!(
            "All finished. Please checkout your files at {}",
            self.output_file.display()
        );
        Ok(())
    }
}

impl<S> ConcatAfterMerger<S>
where
    S: StreamingSegment + ToSegmentData + Send + Sync + 'static,
{
    fn segment_path(&self, segment: &S) -> PathBuf {
        segment_path(segment, &self.temp_dir)
    }
}

struct ConcatSegment<S>(S, bool /* success */);

async fn concat_merge<S, P, O>(
    segments: &mut Vec<ConcatSegment<S>>,
    cwd: P,
    output_path: O,
) -> IoriResult<()>
where
    S: StreamingSegment,
    P: AsRef<Path>,
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
                    .with_extension(format!(".{file_count}.{file_extension}")),
            )
            .await?;
        }

        let filename = format!("{:06}_{}", segment.sequence(), segment.file_name());
        let path = cwd.as_ref().join(filename);
        let mut file = File::open(path).await?;
        tokio::io::copy(&mut file, &mut output).await?;
    }
    Ok(())
}

pub fn segment_path<S, P>(segment: &S, cwd: P) -> PathBuf
where
    S: StreamingSegment,
    P: AsRef<Path>,
{
    let filename = segment.file_name();
    let sequence = segment.sequence();
    let filename = format!("{sequence:06}_{filename}");
    cwd.as_ref().join(filename)
}
