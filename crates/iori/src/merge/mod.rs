use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::{
    dash::{DashSegmentInfo, DashSegmentType},
    error::IoriResult,
};

pub async fn mkvmerge_dash<P>(
    segments: Vec<DashSegmentInfo>,
    cwd: P,
    output: PathBuf,
) -> IoriResult<()>
where
    P: AsRef<Path>,
{
    let videos = segments
        .iter()
        .filter(|info| info.r#type == DashSegmentType::Video);
    let audios = segments
        .iter()
        .filter(|info| info.r#type == DashSegmentType::Audio);

    // 1. merge videos with mkvmerge
    let mut video = Command::new("mkvmerge")
        .current_dir(&cwd)
        .arg("-q")
        .arg("[")
        .args(videos.map(|info| {
            let filename = format!("{:06}_{}", info.sequence, info.filename);
            filename
        }))
        .arg("]")
        .arg("-o")
        .arg("iori_video.mkv")
        .spawn()?;

    // 2. merge audios with mkvmerge
    let mut audio = Command::new("mkvmerge")
        .current_dir(&cwd)
        .arg("-q")
        .arg("[")
        .args(audios.map(|info| {
            let filename = format!("{:06}_{}", info.sequence, info.filename);
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
        .arg(output)
        .spawn()?;
    merge.wait().await?;

    // 4. remove temporary files
    tokio::fs::remove_file(cwd.as_ref().join("iori_video.mkv")).await?;
    tokio::fs::remove_file(cwd.as_ref().join("iori_audio.mkv")).await?;

    Ok(())
}
