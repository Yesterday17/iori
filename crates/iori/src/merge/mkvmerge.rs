use crate::{
    cache::CacheSource, error::IoriResult, util::file_name_add_suffix, SegmentInfo, SegmentType,
};
use std::path::{Path, PathBuf};
use tokio::process::Command;

use super::{concat::ConcatSegment, Merger};

pub struct MkvMergeMerver {
    segments: Vec<ConcatSegment>,

    /// Final output file path.
    output_file: PathBuf,
    /// Keep downloaded segments after merging.
    keep_segments: bool,

    has_failed: bool,
}

impl MkvMergeMerver {
    pub fn new(output_file: PathBuf, keep_segments: bool) -> Self {
        Self {
            segments: Vec::new(),
            output_file,
            keep_segments,
            has_failed: false,
        }
    }
}

impl Merger for MkvMergeMerver {
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
        self.has_failed = true;
        Ok(())
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
        log::info!("Merging chunks...");

        if self.has_failed {
            log::warn!("Some segments failed to download. Skipping merging.");
            if let Some(location) = cache.location_hint() {
                log::warn!("You can find the downloaded segments at {location}");
            }
            return Ok(());
        }

        let segments: Vec<_> = self.segments.iter().map(|s| &s.segment).collect();
        mkvmerge_merge(segments, &cache, &self.output_file).await?;

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

pub async fn mkvmerge_merge<O>(
    segments: Vec<&SegmentInfo>,
    cache: &impl CacheSource,
    output: O,
) -> IoriResult<()>
where
    O: AsRef<Path>,
{
    let mut tracks = Vec::new();

    // TODO: group by stream id instead of segment type

    // 1. merge videos with mkvmerge
    let mut videos: Vec<_> = segments
        .iter()
        .filter(|info| info.r#type == SegmentType::Video)
        .collect();
    if !videos.is_empty() {
        videos.sort_by(|a, b| a.sequence.cmp(&b.sequence));
        let mut paths = Vec::with_capacity(videos.len());
        for video_segment in videos {
            let filename = cache.segment_path(video_segment).await.unwrap();
            paths.push(filename);
        }

        let mut video_path = output.as_ref().to_owned();
        file_name_add_suffix(&mut video_path, "iori_video");
        video_path.set_extension("mp4");

        let mut video = Command::new("mkvmerge")
            .arg("-q")
            .arg("[")
            .args(paths)
            .arg("]")
            .arg("-o")
            .arg(&video_path)
            .spawn()?;
        video.wait().await?;
        tracks.push(video_path);
    }

    // 2. merge audios with mkvmerge
    let mut audios: Vec<_> = segments
        .iter()
        .filter(|info| info.r#type == SegmentType::Audio)
        .collect();
    if !audios.is_empty() {
        audios.sort_by(|a, b| a.sequence.cmp(&b.sequence));
        let mut paths = Vec::with_capacity(audios.len());
        for audio_segment in audios {
            let filename = cache.segment_path(audio_segment).await.unwrap();
            paths.push(filename);
        }

        let mut audio_path = output.as_ref().to_owned();
        file_name_add_suffix(&mut audio_path, "iori_audio");
        audio_path.set_extension("m4a");

        let mut audio = Command::new("mkvmerge")
            .arg("-q")
            .arg("[")
            .args(paths)
            .arg("]")
            .arg("-o")
            .arg(&audio_path)
            .spawn()?;
        audio.wait().await?;
        tracks.push(audio_path);
    }

    // 3. merge audio and video
    let mut merge = Command::new("mkvmerge")
        .args(tracks.iter())
        .arg("-o")
        .arg(output.as_ref())
        .spawn()?;
    merge.wait().await?;

    // 4. remove temporary files
    for track in tracks {
        tokio::fs::remove_file(track).await?;
    }

    Ok(())
}
