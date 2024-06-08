use std::{ffi::OsStr, path::Path};

use tokio::{fs::File, process::Command};

use crate::{common::SegmentType, error::IoriResult};

pub trait MergableSegmentInfo {
    fn sequence(&self) -> u64;

    fn file_name(&self) -> &str;

    fn r#type(&self) -> SegmentType;
}

impl MergableSegmentInfo for Box<dyn MergableSegmentInfo> {
    fn sequence(&self) -> u64 {
        self.as_ref().sequence()
    }

    fn file_name(&self) -> &str {
        self.as_ref().file_name()
    }

    fn r#type(&self) -> SegmentType {
        self.as_ref().r#type()
    }
}

pub async fn merge<S, P, O>(segments: Vec<S>, cwd: P, output: O) -> IoriResult<()>
where
    S: MergableSegmentInfo,
    P: AsRef<Path>,
    O: AsRef<Path>,
{
    // if more than one type of segment is present, use mkvmerge
    let has_video = segments
        .iter()
        .any(|info| info.r#type() == SegmentType::Video);
    let has_audio = segments
        .iter()
        .any(|info| info.r#type() == SegmentType::Audio);
    if has_video && has_audio {
        mkvmerge_merge(segments, cwd, output).await?;
        return Ok(());
    }

    // if file is mpegts, use concat
    let is_segments_mpegts = segments
        .iter()
        .all(|info| info.file_name().to_lowercase().ends_with(".ts"));
    let is_output_mpegts = output.as_ref().extension() == Some(OsStr::new("ts"));
    if is_segments_mpegts && is_output_mpegts {
        concat_merge(segments, cwd, output).await?;
        return Ok(());
    }

    // use mkvmerge as fallback
    mkvmerge_merge(segments, cwd, output).await?;

    Ok(())
}

pub async fn concat_merge<S, P, O>(mut segments: Vec<S>, cwd: P, output: O) -> IoriResult<()>
where
    S: MergableSegmentInfo,
    P: AsRef<Path>,
    O: AsRef<Path>,
{
    segments.sort_by(|a, b| a.sequence().cmp(&b.sequence()));

    let mut output = File::create(output).await?;
    for segment in segments {
        let filename = format!("{:06}_{}", segment.sequence(), segment.file_name());
        let path = cwd.as_ref().join(filename);
        let mut file = File::open(path).await?;
        tokio::io::copy(&mut file, &mut output).await?;
    }
    Ok(())
}

pub async fn mkvmerge_merge<S, P, O>(segments: Vec<S>, cwd: P, output: O) -> IoriResult<()>
where
    S: MergableSegmentInfo,
    P: AsRef<Path>,
    O: AsRef<Path>,
{
    // 1. merge videos with mkvmerge
    let mut videos: Vec<_> = segments
        .iter()
        .filter(|info| info.r#type() == SegmentType::Video)
        .collect();
    videos.sort_by(|a, b| a.sequence().cmp(&b.sequence()));
    let mut video = Command::new("mkvmerge")
        .current_dir(&cwd)
        .arg("-q")
        .arg("[")
        .args(videos.iter().map(|info| {
            let filename = format!("{:06}_{}", info.sequence(), info.file_name());
            filename
        }))
        .arg("]")
        .arg("-o")
        .arg("iori_video.mkv")
        .spawn()?;

    // 2. merge audios with mkvmerge
    let mut audios: Vec<_> = segments
        .iter()
        .filter(|info| info.r#type() == SegmentType::Audio)
        .collect();
    audios.sort_by(|a, b| a.sequence().cmp(&b.sequence()));
    let mut audio = Command::new("mkvmerge")
        .current_dir(&cwd)
        .arg("-q")
        .arg("[")
        .args(audios.iter().map(|info| {
            let filename = format!("{:06}_{}", info.sequence(), info.file_name());
            filename
        }))
        .arg("]")
        .arg("-o")
        .arg("iori_audio.mkv")
        .spawn()?;

    video.wait().await?;
    audio.wait().await?;

    // 3. merge audio and video
    let mut merge = Command::new("mkvmerge")
        .current_dir(&cwd)
        .arg("iori_video.mkv")
        .arg("iori_audio.mkv")
        .arg("-o")
        .arg(output.as_ref())
        .spawn()?;
    merge.wait().await?;

    // 4. remove temporary files
    tokio::fs::remove_file(cwd.as_ref().join("iori_video.mkv")).await?;
    tokio::fs::remove_file(cwd.as_ref().join("iori_audio.mkv")).await?;

    Ok(())
}
