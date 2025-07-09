use crate::{cache::CacheSource, error::IoriResult, SegmentInfo};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use tokio::process::Command;

/// Concatenate segments using ffmpeg CLI concat demuxer.
///
/// This function creates a temporary file list and uses ffmpeg's concat demuxer
/// to concatenate the segments. This is more efficient than using the concat protocol
/// for many files.
#[allow(unused)]
pub(crate) async fn ffmpeg_cli_concat<O>(
    segments: &[&SegmentInfo],
    cache: &impl CacheSource,
    output_path: O,
) -> IoriResult<()>
where
    O: AsRef<Path>,
{
    if segments.is_empty() {
        return Ok(());
    }

    tracing::debug!("Concatenating with ffmpeg CLI...");

    let ffmpeg = which::which("ffmpeg")?;

    // Create a temporary file list for ffmpeg concat demuxer
    let mut temp = tempfile::Builder::new().suffix(".txt").tempfile()?;
    for segment in segments {
        let filename = cache.segment_path(segment).await.unwrap();
        writeln!(temp, "file '{}'", filename.to_string_lossy())?;
    }
    temp.flush()?;

    let mut child = Command::new(ffmpeg)
        .args(["-f", "concat", "-safe", "0", "-i"])
        .arg(temp.path())
        .args(["-c", "copy"])
        .arg(output_path.as_ref())
        .spawn()?;
    child.wait().await?;

    Ok(())
}

/// Merge multiple tracks into a single output file using ffmpeg CLI.
///
/// This function takes multiple track files and merges them into a single output file,
/// mapping all streams and using stream copy to avoid re-encoding.
#[allow(unused)]
pub(crate) async fn ffmpeg_cli_merge<O>(tracks: Vec<PathBuf>, output: O) -> IoriResult<()>
where
    O: AsRef<Path>,
{
    assert!(tracks.len() > 1);

    tracing::debug!("Merging with ffmpeg CLI...");

    let ffmpeg = which::which("ffmpeg")?;
    let mut command = Command::new(ffmpeg);
    
    // Add input files
    for track in &tracks {
        command.args(["-i", &track.to_string_lossy()]);
    }

    // Map all streams and use copy codec
    for i in 0..tracks.len() {
        command.args(["-map", &i.to_string()]);
    }
    
    command
        .args(["-c", "copy"])
        .arg(output.as_ref())
        .spawn()?
        .wait()
        .await?;

    // remove temporary files
    for track in tracks {
        tokio::fs::remove_file(track).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ffmpeg_available() {
        // Test that we can detect ffmpeg binary
        // This test will pass if ffmpeg is available in PATH, skip otherwise
        match which::which("ffmpeg") {
            Ok(path) => {
                assert!(path.exists());
            }
            Err(_) => {
                // ffmpeg not available, skip test
                println!("ffmpeg not available in PATH, skipping test");
            }
        }
    }
}