use std::{path::PathBuf, sync::Arc, time::Duration};

use tokio::{
    io::AsyncWrite,
    sync::{mpsc, Mutex},
};
use url::Url;

use crate::{
    error::{IoriError, IoriResult},
    fetch::fetch_segment,
    hls::{segment::M3u8Segment, source::AdvancedM3u8Source},
    util::{http::HttpClient, mix::mix_vec},
    StreamingSource,
};

pub struct CommonM3u8LiveSource {
    client: HttpClient,
    playlist: Arc<Mutex<AdvancedM3u8Source>>,
    retry: u32,
    shaka_packager_command: Option<PathBuf>,
}

impl CommonM3u8LiveSource {
    pub fn new(
        client: HttpClient,
        m3u8_url: String,
        key: Option<&str>,
        shaka_packager_command: Option<PathBuf>,
    ) -> Self {
        Self {
            client: client.clone(),
            playlist: Arc::new(Mutex::new(AdvancedM3u8Source::new(
                client,
                Url::parse(&m3u8_url).unwrap(),
                key,
                3,
            ))),
            shaka_packager_command,
            retry: 3,
        }
    }

    pub fn with_retry(mut self, retry: u32) -> Self {
        self.retry = retry;
        self
    }
}

impl StreamingSource for CommonM3u8LiveSource {
    type Segment = M3u8Segment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let mut latest_media_sequences =
            self.playlist.lock().await.load_streams(self.retry).await?;

        let (sender, receiver) = mpsc::unbounded_channel();

        let retry = self.retry;
        let playlist = self.playlist.clone();
        tokio::spawn(async move {
            loop {
                if sender.is_closed() {
                    break;
                }

                let before_load = tokio::time::Instant::now();
                let (segments, is_end) = match playlist
                    .lock()
                    .await
                    .load_segments(&latest_media_sequences, retry)
                    .await
                {
                    Ok(v) => v,
                    Err(IoriError::M3u8FetchError) => {
                        log::error!("Exceeded retry limit for fetching segments, exiting...");
                        break;
                    }
                    Err(e) => {
                        log::error!("Failed to fetch segments: {e}");
                        break;
                    }
                };

                let segments_average_duration = segments
                    .iter()
                    .map(|ss| {
                        let total_duration = ss.iter().map(|s| s.duration).sum::<f32>();
                        let segments_count = ss.len() as f32;

                        if segments_count == 0. {
                            0
                        } else {
                            (total_duration / segments_count) as u64
                        }
                    })
                    .min()
                    .unwrap_or(5);

                for (segments, latest_media_sequence) in
                    segments.iter().zip(latest_media_sequences.iter_mut())
                {
                    *latest_media_sequence = segments
                        .last()
                        .map(|r| r.media_sequence)
                        .or_else(|| latest_media_sequence.clone());
                }

                let mixed_segments = mix_vec(segments);
                if !mixed_segments.is_empty() {
                    if let Err(_) = sender.send(Ok(mixed_segments)) {
                        break;
                    }
                }

                if is_end {
                    break;
                }

                // playlist does not end, wait for a while and fetch again
                let seconds_to_wait = segments_average_duration.clamp(1, 5);
                tokio::time::sleep_until(before_load + Duration::from_secs(seconds_to_wait)).await;
            }
        });

        Ok(receiver)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        fetch_segment(
            self.client.clone(),
            segment,
            writer,
            self.shaka_packager_command.clone(),
        )
        .await?;
        Ok(())
    }
}
