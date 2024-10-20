use std::path::{Path, PathBuf};

use tokio::fs::File;

use crate::{error::IoriResult, StreamingSegment};

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

pub async fn open_writer<S, P>(segment: &S, cwd: P) -> IoriResult<Option<File>>
where
    S: StreamingSegment,
    P: AsRef<Path>,
{
    let path = segment_path(segment, cwd);
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
