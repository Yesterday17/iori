mod concat;
mod mkvmerge;
mod pipe;
mod skip;

pub use concat::ConcatAfterMerger;
pub use pipe::PipeMerger;
pub use skip::SkipMerger;

use crate::{error::IoriResult, StreamingSegment, ToSegmentData};
use std::{future::Future, path::PathBuf};
use tokio::{fs::File, io::AsyncWrite};

pub trait Merger {
    /// Segment to be merged
    type Segment: StreamingSegment + ToSegmentData + Send + Sync + 'static;
    type MergeSegment: AsyncWrite + Unpin + Send + Sync + 'static;
    /// Result of the merge
    type MergeResult: Send + Sync + 'static;

    /// Open a writer for the merged file.
    fn open_writer(
        &self,
        segment: &Self::Segment,
    ) -> impl std::future::Future<Output = IoriResult<Option<Self::MergeSegment>>> + Send;

    /// Add a segment to the merger.
    ///
    /// This method might not be called in order of segment sequence.
    /// Implementations should handle order of segments by calling
    /// [StreamingSegment::sequence].
    fn update(&mut self, segment: Self::Segment) -> impl Future<Output = IoriResult<()>> + Send;

    /// Tell the merger that a segment has failed to download.
    fn fail(&mut self, segment: Self::Segment) -> impl Future<Output = IoriResult<()>> + Send;

    fn finish(&mut self)
        -> impl std::future::Future<Output = IoriResult<Self::MergeResult>> + Send;
}

pub enum IoriMerger<S> {
    Pipe(PipeMerger<S>),
    Skip(SkipMerger<S>),
    Concat(ConcatAfterMerger<S>),
}

impl<S> IoriMerger<S> {
    pub fn pipe<P>(output_dir: P, recycle: bool) -> IoriResult<Self>
    where
        P: Into<PathBuf>,
    {
        Ok(Self::Pipe(PipeMerger::new(output_dir, recycle)?))
    }

    pub fn skip<P>(output_dir: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self::Skip(SkipMerger::new(output_dir))
    }

    pub fn concat<T>(temp_dir: T, output_file: PathBuf) -> Self
    where
        T: Into<PathBuf>,
    {
        Self::Concat(ConcatAfterMerger::new(temp_dir, output_file))
    }
}

impl<S> Merger for IoriMerger<S>
where
    S: StreamingSegment + ToSegmentData + Send + Sync + 'static,
{
    type Segment = S;
    type MergeSegment = File; // TODO: not all mergers need to write to file
    type MergeResult = (); // TODO: merger might have different result types

    async fn open_writer(&self, segment: &Self::Segment) -> IoriResult<Option<Self::MergeSegment>> {
        match self {
            Self::Pipe(merger) => merger.open_writer(segment).await,
            Self::Skip(merger) => merger.open_writer(segment).await,
            Self::Concat(merger) => merger.open_writer(segment).await,
        }
    }

    async fn update(&mut self, segment: Self::Segment) -> IoriResult<()> {
        match self {
            Self::Pipe(merger) => merger.update(segment).await,
            Self::Skip(merger) => merger.update(segment).await,
            Self::Concat(merger) => merger.update(segment).await,
        }
    }

    async fn fail(&mut self, segment: Self::Segment) -> IoriResult<()> {
        match self {
            Self::Pipe(merger) => merger.fail(segment).await,
            Self::Skip(merger) => merger.fail(segment).await,
            Self::Concat(merger) => merger.fail(segment).await,
        }
    }

    async fn finish(&mut self) -> IoriResult<Self::MergeResult> {
        match self {
            Self::Pipe(merger) => merger.finish().await,
            Self::Skip(merger) => merger.finish().await,
            Self::Concat(merger) => merger.finish().await,
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

// pub async fn mkvmerge_merge<S, P, O>(segments: Vec<S>, cwd: P, output: O) -> IoriResult<()>
// where
//     S: StreamingSegment,
//     P: AsRef<Path>,
//     O: AsRef<Path>,
// {
//     let mut tracks = Vec::new();

//     // 1. merge videos with mkvmerge
//     let mut videos: Vec<_> = segments
//         .iter()
//         .filter(|info| info.r#type() == SegmentType::Video)
//         .collect();
//     if !videos.is_empty() {
//         videos.sort_by(|a, b| a.sequence().cmp(&b.sequence()));
//         let video_path = cwd.as_ref().join("iori_video.mkv");

//         let mut video = Command::new("mkvmerge")
//             .current_dir(&cwd)
//             .arg("-q")
//             .arg("[")
//             .args(videos.iter().map(|info| {
//                 let filename = format!("{:06}_{}", info.sequence(), info.file_name());
//                 filename
//             }))
//             .arg("]")
//             .arg("-o")
//             .arg(&video_path)
//             .spawn()?;
//         video.wait().await?;
//         tracks.push(video_path);
//     }

//     // 2. merge audios with mkvmerge
//     let mut audios: Vec<_> = segments
//         .iter()
//         .filter(|info| info.r#type() == SegmentType::Audio)
//         .collect();
//     if !audios.is_empty() {
//         audios.sort_by(|a, b| a.sequence().cmp(&b.sequence()));
//         let audio_path = cwd.as_ref().join("iori_audio.mkv");

//         let mut audio = Command::new("mkvmerge")
//             .current_dir(&cwd)
//             .arg("-q")
//             .arg("[")
//             .args(audios.iter().map(|info| {
//                 let filename = format!("{:06}_{}", info.sequence(), info.file_name());
//                 filename
//             }))
//             .arg("]")
//             .arg("-o")
//             .arg(&audio_path)
//             .spawn()?;
//         audio.wait().await?;
//         tracks.push(audio_path);
//     }

//     // 3. merge audio and video
//     let mut merge = Command::new("mkvmerge")
//         .current_dir(&cwd)
//         .args(tracks.iter())
//         .arg("-o")
//         .arg(output.as_ref())
//         .spawn()?;
//     merge.wait().await?;

//     // 4. remove temporary files
//     for track in tracks {
//         tokio::fs::remove_file(track).await?;
//     }

//     Ok(())
// }
