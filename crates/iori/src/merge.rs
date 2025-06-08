mod auto;
mod concat;
#[cfg(feature = "ffmpeg")]
mod ffmpeg;
mod pipe;
mod skip;

pub use auto::AutoMerger;
pub use concat::ConcatAfterMerger;
pub use pipe::PipeMerger;
pub use skip::SkipMerger;
use tokio::io::AsyncWrite;

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
    Auto(AutoMerger),
}

impl IoriMerger {
    pub fn pipe(recycle: bool) -> Self {
        Self::Pipe(PipeMerger::stdout(recycle))
    }

    pub fn pipe_to_writer(
        recycle: bool,
        writer: impl AsyncWrite + Unpin + Send + Sync + 'static,
    ) -> Self {
        Self::Pipe(PipeMerger::writer(recycle, writer))
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

    pub fn auto(output_file: PathBuf, keep_segments: bool) -> Self {
        Self::Auto(AutoMerger::new(output_file, keep_segments))
    }
}

impl Merger for IoriMerger {
    type Result = (); // TODO: merger might have different result types

    async fn update(&mut self, segment: SegmentInfo, cache: impl CacheSource) -> IoriResult<()> {
        match self {
            Self::Pipe(merger) => merger.update(segment, cache).await,
            Self::Skip(merger) => merger.update(segment, cache).await,
            Self::Concat(merger) => merger.update(segment, cache).await,
            Self::Auto(merger) => merger.update(segment, cache).await,
        }
    }

    async fn fail(&mut self, segment: SegmentInfo, cache: impl CacheSource) -> IoriResult<()> {
        match self {
            Self::Pipe(merger) => merger.fail(segment, cache).await,
            Self::Skip(merger) => merger.fail(segment, cache).await,
            Self::Concat(merger) => merger.fail(segment, cache).await,
            Self::Auto(merger) => merger.fail(segment, cache).await,
        }
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
        match self {
            Self::Pipe(merger) => merger.finish(cache).await,
            Self::Skip(merger) => merger.finish(cache).await,
            Self::Concat(merger) => merger.finish(cache).await,
            Self::Auto(merger) => merger.finish(cache).await,
        }
    }
}
