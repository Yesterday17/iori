use super::{concat::ConcatMergeNamer, Merger};
use crate::{
    cache::CacheSource, error::IoriResult, util::ordered_stream::OrderedStream, SegmentType,
    StreamingSegment,
};
use command_fds::{CommandFdExt, FdMapping};
use std::{future::Future, path::PathBuf, pin::Pin, process::Stdio};
use tokio::{io::AsyncRead, net::unix::pipe::pipe, process::Command, sync::mpsc, task::JoinHandle};

type SendSegment = (
    Pin<Box<dyn AsyncRead + Send + 'static>>,
    SegmentType,
    Pin<Box<dyn Future<Output = IoriResult<()>> + Send>>,
);

pub struct PipeMerger {
    recycle: bool,

    sender: Option<mpsc::UnboundedSender<(u64, Option<SendSegment>)>>,
    future: Option<JoinHandle<()>>,
}

impl PipeMerger {
    pub fn stdout(recycle: bool) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut stream: OrderedStream<Option<SendSegment>> = OrderedStream::new(rx);
        let future = tokio::spawn(async move {
            let mut stdout = tokio::io::stdout();
            while let Some(segment) = stream.next().await {
                if let Some((mut reader, _type, invalidate)) = segment {
                    _ = tokio::io::copy(&mut reader, &mut stdout).await;
                    if recycle {
                        _ = invalidate.await;
                    }
                }
            }
        });

        Self {
            recycle,

            sender: Some(tx),
            future: Some(future),
        }
    }

    pub fn file(recycle: bool, target_path: PathBuf) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut stream: OrderedStream<Option<SendSegment>> = OrderedStream::new(rx);
        let future = tokio::spawn(async move {
            let mut namer = ConcatMergeNamer::new(&target_path);
            let mut target = Some(
                tokio::fs::File::create(&target_path)
                    .await
                    .expect("Failed to create file"),
            );
            while let Some(segment) = stream.next().await {
                if let Some((mut reader, _type, invalidate)) = segment {
                    if target.is_none() {
                        let file = tokio::fs::File::create(namer.next())
                            .await
                            .expect("Failed to create file");
                        target = Some(file);
                    }

                    if let Some(target) = &mut target {
                        _ = tokio::io::copy(&mut reader, target).await;
                    }
                    if recycle {
                        _ = invalidate.await;
                    }
                } else {
                    target = None;
                }
            }
        });

        Self {
            recycle,

            sender: Some(tx),
            future: Some(future),
        }
    }

    pub fn mux(recycle: bool, output: PathBuf, extra_command: Option<String>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut stream: OrderedStream<Option<SendSegment>> = OrderedStream::new(rx);

        let (mut audio_pipe, audio_receiver) = pipe().unwrap();
        let audio_receiver = audio_receiver.into_blocking_fd().unwrap();

        let future = tokio::spawn(async move {
            // TODO: maybe creating a new process might be better
            let mut video_pipe = tokio::spawn(async move {
                let mut command = Command::new("ffmpeg");
                command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .fd_mappings(vec![FdMapping {
                        parent_fd: audio_receiver,
                        child_fd: 3,
                    }])
                    .unwrap();
                command.args(["-y", "-fflags", "+genpts"]); // , "-loglevel", "quiet"

                if extra_command.is_some() {
                    command.arg("-re");
                }

                // // #[rustfmt::skip]
                command.args([
                    "-i",
                    "pipe:0",
                    "-i",
                    "pipe:3",
                    "-map",
                    "0",
                    "-map",
                    "1",
                    "-strict",
                    "unofficial",
                    "-c",
                    "copy",
                    "-metadata",
                    &format!(r#"date="{}""#, chrono::Utc::now().to_rfc3339()),
                    "-ignore_unknown",
                    "-copy_unknown",
                ]);

                if let Some(dest) = extra_command.and_then(|s| shlex::split(&s)) {
                    command.args(dest);
                } else {
                    command.args(["-f", "mpegts", "-shortest"]).arg(output);
                }

                let mut process = command.spawn().unwrap();
                let stdin = process.stdin.take().unwrap();
                tokio::spawn(async move {
                    process.wait().await.unwrap();
                });

                stdin
            })
            .await
            .unwrap();

            let (video_sender, mut video_receiver) = mpsc::unbounded_channel::<SendSegment>();
            let video_handle = tokio::spawn(async move {
                while let Some((mut reader, _, invalidate)) = video_receiver.recv().await {
                    tokio::io::copy(&mut reader, &mut video_pipe).await.unwrap();
                    invalidate.await.unwrap();
                }
            });

            let (audio_sender, mut audio_receiver) = mpsc::unbounded_channel::<SendSegment>();
            let audio_handle = tokio::spawn(async move {
                while let Some((mut reader, _, invalidate)) = audio_receiver.recv().await {
                    tokio::io::copy(&mut reader, &mut audio_pipe).await.unwrap();
                    invalidate.await.unwrap();
                }
            });

            while let Some(segment) = stream.next().await {
                if let Some((reader, r#type, invalidate)) = segment {
                    match r#type {
                        SegmentType::Video => {
                            video_sender.send((reader, r#type, invalidate)).unwrap();
                        }
                        SegmentType::Audio => {
                            audio_sender.send((reader, r#type, invalidate)).unwrap();
                        }
                        SegmentType::Subtitle => {
                            if recycle {
                                _ = invalidate.await;
                            }
                        }
                    }
                }
            }

            video_handle.await.unwrap();
            audio_handle.await.unwrap();
        });

        Self {
            recycle,

            sender: Some(tx),
            future: Some(future),
        }
    }

    fn send(&self, message: (u64, Option<SendSegment>)) {
        if let Some(sender) = &self.sender {
            sender.send(message).expect("Failed to send segment");
        }
    }
}

impl Merger for PipeMerger {
    type Result = ();

    async fn update(
        &mut self,
        segment: impl StreamingSegment + Send + Sync + 'static,
        cache: impl CacheSource,
    ) -> IoriResult<()> {
        let sequence = segment.sequence();
        let r#type = segment.r#type();
        let reader = cache.open_reader(&segment).await?;
        let invalidate = async move { cache.invalidate(&segment).await };

        self.send((
            sequence,
            Some((Box::pin(reader), r#type, Box::pin(invalidate))),
        ));

        Ok(())
    }

    async fn fail(
        &mut self,
        segment: impl StreamingSegment + Send + Sync + 'static,
        cache: impl CacheSource,
    ) -> IoriResult<()> {
        cache.invalidate(&segment).await?;

        self.send((segment.sequence(), None));

        Ok(())
    }

    async fn finish(&mut self, cache: impl CacheSource) -> IoriResult<Self::Result> {
        // drop the sender so that the future can finish
        drop(self.sender.take());

        self.future
            .take()
            .unwrap()
            .await
            .expect("Failed to join pipe");

        if self.recycle {
            cache.clear().await?;
        }

        Ok(())
    }
}
