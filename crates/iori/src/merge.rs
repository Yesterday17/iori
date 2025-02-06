mod concat;
mod mkvmerge;
mod pipe;
mod skip;

pub use concat::ConcatAfterMerger;
pub use mkvmerge::MkvMergeMerver;
pub use pipe::PipeMerger;
pub use skip::SkipMerger;

use crate::{cache::CacheSource, error::IoriResult, SegmentInfo};
use std::{future::Future, path::PathBuf};

pub trait Merger {
    /// Result of the merge.
    type Result: Send + Sync + 'static;

    /// Add a segment to the merger.
    ///
    /// This method might not be called in order of segment sequence.
    /// Implementations should handle order of segments by calling
    /// [StreamingSegment::sequence].
    fn update(
        &mut self,
        segment: SegmentInfo,
        cache: impl CacheSource,
    ) -> impl Future<Output = IoriResult<()>> + Send;

    /// Tell the merger that a segment has failed to download.
    fn fail(
        &mut self,
        segment: SegmentInfo,
        cache: impl CacheSource,
    ) -> impl Future<Output = IoriResult<()>> + Send;

    fn finish(
        &mut self,
        cache: impl CacheSource,
    ) -> impl std::future::Future<Output = IoriResult<Self::Result>> + Send;
}

pub enum IoriMerger {
    Pipe(PipeMerger),
    Skip(SkipMerger),
    Concat(ConcatAfterMerger),
    MkvMerge(MkvMergeMerver),
}

impl IoriMerger {
    pub fn pipe(recycle: bool) -> Self {
        Self::Pipe(PipeMerger::stdout(recycle))
    }

    pub fn pipe_to_file(recycle: bool, output_file: PathBuf) -> Self {
        Self::Pipe(PipeMerger::file(recycle, output_file))
    }

    pub fn pipe_mux(recycle: bool, output_file: PathBuf, extra_commands: Option<String>) -> Self {
        Self::Pipe(PipeMerger::mux(recycle, output_file, extra_commands))
    }

    pub fn skip() -> Self {
        Self::Skip(SkipMerger)
    }

    pub fn concat(output_file: PathBuf, keep_segments: bool) -> Self {
        Self::Concat(ConcatAfterMerger::new(output_file, keep_segments))
    }

    pub fn mkvmerge(output_file: PathBuf, keep_segments: bool) -> Self {
        Self::MkvMerge(MkvMergeMerver::new(output_file, keep_segments))
    }
}

impl Merger for IoriMerger {
    type Result = (); // TODO: merger might have different result types

    async fn update(&mut self, segment: SegmentInfo, cache: impl CacheSource) -> IoriResult<()> {
        match self {
            Self::Pipe(merger) => merger.update(segment, cache).await,
            Self::Skip(merger) => merger.update(segment, cache).await,
            Self::Concat(merger) => merger.update(segment, cache).await,
            Self::MkvMerge(merger) => merger.update(segment, cache).await,
        }
    }

    async fn fail(&mut self, segment: SegmentInfo, cache: impl CacheSource) -> IoriResult<()> {
        match self {
            Self::Pipe(merger) => merger.fail(segment, cache).await,
            Self::Skip(merger) => merger.fail(segment, cache).await,
            Self::Concat(merger) => merger.fail(segment, cache).await,
            Self::MkvMerge(merger) => merger.fail(segment, cache).await,
        }
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
        match self {
            Self::Pipe(merger) => merger.finish(cache).await,
            Self::Skip(merger) => merger.finish(cache).await,
            Self::Concat(merger) => merger.finish(cache).await,
            Self::MkvMerge(merger) => merger.finish(cache).await,
        }
    }
}

// pub async fn merge<S, P, O>(segments: Vec<S>, cwd: P, output: O) -> IoriResult<()>
// where
//     S: StreamingSegment,
//     P: AsRef<Path>,
//     O: AsRef<Path>,
// {
//     // if more than one type of segment is present, use mkvmerge
//     let has_video = segments
//         .iter()
//         .any(|info| info.r#type() == SegmentType::Video);
//     let has_audio = segments
//         .iter()
//         .any(|info| info.r#type() == SegmentType::Audio);
//     if has_video && has_audio {
//         mkvmerge_merge(segments, cwd, output).await?;
//         return Ok(());
//     }

//     // if file is mpegts, use concat
//     let is_segments_mpegts = segments
//         .iter()
//         .all(|info| info.file_name().to_lowercase().ends_with(".ts"));
//     let is_output_mpegts = output.as_ref().extension() == Some(OsStr::new("ts"));
//     if is_segments_mpegts && is_output_mpegts {
//         concat_merge(segments, cwd, output).await?;
//         return Ok(());
//     }

//     // use mkvmerge as fallback
//     mkvmerge_merge(segments, cwd, output).await?;

//     Ok(())
// }
