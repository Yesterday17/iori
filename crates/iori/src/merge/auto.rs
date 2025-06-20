use crate::{
    cache::CacheSource, error::IoriResult, util::path::IoriPathExt, SegmentFormat, SegmentInfo,
    SegmentType,
};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
};
use tokio::{fs::File, io::BufWriter, process::Command};

use super::{concat::ConcatSegment, Merger};

/// AutoMerger is a merger that automatically chooses the best strategy to merge segments.
///
/// For MPEG-TS:
/// - It will use concat to merge segments.
/// - If there is only one track, the behavior is the same as [ConcatAfterMerger].
///
/// For other formats:
/// - It will use mkvmerge to merge segments.
///
/// If there are multiple tracks to merge, it will use mkvmerge to merge them.
/// If there are any missing segments, the merge will be skipped.
pub struct AutoMerger {
    segments: HashMap<u64, Vec<ConcatSegment>>,

    /// Keep downloaded segments after merging.
    keep_segments: bool,

    has_failed: bool,

    /// Final output file path. It may not have an extension.
    output_file: PathBuf,
    /// A list of file extensions which should skip adding an auto extension.
    allowed_extensions: Vec<&'static str>,
}

impl AutoMerger {
    pub fn new(output_file: PathBuf, keep_segments: bool) -> Self {
        Self {
            segments: HashMap::new(),
            keep_segments,
            has_failed: false,

            output_file,
            allowed_extensions: vec!["mkv", "mp4", "ts"],
        }
    }
}

impl Merger for AutoMerger {
    type Result = ();

    async fn update(&mut self, segment: SegmentInfo, _cache: impl CacheSource) -> IoriResult<()> {
        self.segments
            .entry(segment.stream_id)
            .or_default()
            .push(ConcatSegment {
                segment,
                success: true,
            });
        Ok(())
    }

    async fn fail(&mut self, segment: SegmentInfo, cache: impl CacheSource) -> IoriResult<()> {
        cache.invalidate(&segment).await?;
        self.segments
            .entry(segment.stream_id)
            .or_default()
            .push(ConcatSegment {
                segment,
                success: false,
            });
        self.has_failed = true;
        Ok(())
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
        tracing::info!("Merging chunks...");

        if self.has_failed {
            tracing::warn!("Some segments failed to download. Skipping merging.");
            if let Some(location) = cache.location_hint() {
                tracing::warn!("You can find the downloaded segments at {location}");
            }
            return Ok(());
        }

        let mut tracks = Vec::new();
        for (stream_id, segments) in self.segments.iter() {
            let mut segments: Vec<_> = segments.iter().map(|s| &s.segment).collect();

            let first_segment = segments[0];
            let mut output_path = self.output_file.to_owned();
            output_path.add_suffix(format!("{stream_id:02}"));
            output_path.set_extension(first_segment.format.as_ext());

            segments.sort_by(|a, b| a.sequence.cmp(&b.sequence));

            let can_concat = segments.iter().all(|s| {
                matches!(
                    s.format,
                    SegmentFormat::Mpeg2TS | SegmentFormat::Aac | SegmentFormat::Raw(_)
                ) || matches!(s.r#type, SegmentType::Subtitle)
            });
            if can_concat {
                concat_merge(&segments, &cache, &output_path).await?;
            } else {
                #[cfg(feature = "ffmpeg")]
                {
                    output_path.set_extension("ts");
                    super::ffmpeg::ffmpeg_concat(&segments, &cache, &output_path).await?;
                }
                #[cfg(not(feature = "ffmpeg"))]
                {
                    output_path.set_extension("mkv");
                    mkvmerge_concat(&segments, &cache, &output_path).await?;
                }
            }

            tracks.push(output_path);
        }

        tracing::info!("Merging streams...");

        let output_path = if tracks.len() == 1 {
            let track_format = tracks[0].extension().and_then(|e| e.to_str());
            let output = match track_format {
                Some(ext) => self
                    .output_file
                    .with_replaced_extension(ext, &self.allowed_extensions),
                None => self.output_file.clone(),
            };
            tokio::fs::rename(&tracks[0], &output).await?;
            output
        } else {
            #[cfg(feature = "ffmpeg")]
            {
                let output = self
                    .output_file
                    .with_replaced_extension("mp4", &self.allowed_extensions);
                super::ffmpeg::ffmpeg_merge(tracks, &output).await?;
                output
            }
            #[cfg(not(feature = "ffmpeg"))]
            {
                let output = self
                    .output_file
                    .with_replaced_extension("mkv", &self.allowed_extensions);
                mkvmerge_merge(tracks, &output).await?;
                output
            }
        };

        if !self.keep_segments {
            tracing::info!("End of merging.");
            tracing::info!("Starting cleaning temporary files.");
            cache.clear().await?;
        }

        tracing::info!(
            "All finished. Please checkout your files at {}",
            output_path.display()
        );
        Ok(())
    }
}

#[allow(unused)]
async fn concat_merge<O>(
    segments: &[&SegmentInfo],
    cache: &impl CacheSource,
    output_path: O,
) -> IoriResult<()>
where
    O: AsRef<Path>,
{
    let output = File::create(output_path.as_ref()).await?;
    let mut output = BufWriter::new(output);
    for segment in segments {
        let mut reader = cache.open_reader(segment).await?;
        tokio::io::copy(&mut reader, &mut output).await?;
    }
    Ok(())
}

#[allow(unused)]
async fn mkvmerge_concat<O>(
    segments: &[&SegmentInfo],
    cache: &impl CacheSource,
    output_path: O,
) -> IoriResult<()>
where
    O: AsRef<Path>,
{
    tracing::debug!("Concatenating with mkvmerge...");

    let mkvmerge = which::which("mkvmerge")?;

    let mut args = vec!["-q".to_string(), "[".to_string()];
    for segment in segments {
        let filename = cache.segment_path(segment).await.unwrap();
        args.push(filename.to_string_lossy().to_string());
    }
    args.push("]".to_string());
    args.push("-o".to_string());
    args.push(output_path.as_ref().to_string_lossy().to_string());

    let mut temp = tempfile::Builder::new().tempfile()?;
    let temp_path = temp.path().to_path_buf();
    temp.write_all(serde_json::to_string(&args)?.as_bytes())?;
    temp.flush()?;

    let mut child = Command::new(mkvmerge)
        .arg(format!("@{}", temp_path.to_string_lossy()))
        .spawn()?;
    child.wait().await?;

    Ok(())
}

#[allow(unused)]
async fn mkvmerge_merge<O>(tracks: Vec<PathBuf>, output: O) -> IoriResult<()>
where
    O: AsRef<Path>,
{
    assert!(tracks.len() > 1);

    let mkvmerge = which::which("mkvmerge")?;
    let mut merge = Command::new(mkvmerge)
        .args(tracks.iter())
        .arg("-o")
        .arg(output.as_ref().with_extension("mkv"))
        .spawn()?;
    merge.wait().await?;

    // remove temporary files
    for track in tracks {
        tokio::fs::remove_file(track).await?;
    }

    Ok(())
}
