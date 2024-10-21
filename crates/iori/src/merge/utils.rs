use crate::StreamingSegment;
use std::path::{Path, PathBuf};

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
