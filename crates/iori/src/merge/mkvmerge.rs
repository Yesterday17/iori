use crate::{error::IoriResult, SegmentType, StreamingSegment};
use std::path::Path;
use tokio::process::Command;

pub async fn mkvmerge_merge<S, P, O>(segments: Vec<S>, cwd: P, output: O) -> IoriResult<()>
where
    S: StreamingSegment,
    P: AsRef<Path>,
    O: AsRef<Path>,
{
    let mut tracks = Vec::new();

    // 1. merge videos with mkvmerge
    let mut videos: Vec<_> = segments
        .iter()
        .filter(|info| info.r#type() == SegmentType::Video)
        .collect();
    if !videos.is_empty() {
        videos.sort_by(|a, b| a.sequence().cmp(&b.sequence()));
        let video_path = cwd.as_ref().join("iori_video.mkv");

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
            .arg(&video_path)
            .spawn()?;
        video.wait().await?;
        tracks.push(video_path);
    }

    // 2. merge audios with mkvmerge
    let mut audios: Vec<_> = segments
        .iter()
        .filter(|info| info.r#type() == SegmentType::Audio)
        .collect();
    if !audios.is_empty() {
        audios.sort_by(|a, b| a.sequence().cmp(&b.sequence()));
        let audio_path = cwd.as_ref().join("iori_audio.mkv");

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
            .arg(&audio_path)
            .spawn()?;
        audio.wait().await?;
        tracks.push(audio_path);
    }

    // 3. merge audio and video
    let mut merge = Command::new("mkvmerge")
        .current_dir(&cwd)
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
