use iori::{
    hls::{segment::M3u8Segment, HlsLiveSource},
    HttpClient, IoriResult, StreamingSource,
};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWrite, sync::mpsc};
use url::Url;

use crate::model::WatchResponse;

#[derive(Debug, Serialize, Deserialize)]
pub struct NicoTimeshiftSegmentInfo {
    sequence: u64,
    file_name: String,
}

pub struct NicoTimeshiftSource {
    inner: HlsLiveSource,
    retry: u32,
}

impl NicoTimeshiftSource {
    pub async fn new(
        client: HttpClient,
        wss_url: String,
        quality: &str,
        chase_play: bool,
    ) -> anyhow::Result<Self> {
        let watcher = crate::watch::WatchClient::new(&wss_url).await?;
        watcher.start_watching(quality, chase_play).await?;

        let stream = loop {
            let msg = watcher.recv().await?;
            if let Some(WatchResponse::Stream(stream)) = msg {
                break stream;
            }
        };

        log::info!("Playlist: {}", stream.uri);
        let url = Url::parse(&stream.uri)?;
        client.add_cookies(stream.cookies.into_cookies(), url);

        // keep seats
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = watcher.recv() => {
                        let Ok(msg) = msg else {
                            break;
                        };
                        log::debug!("message: {:?}", msg);
                    }
                    _ = watcher.keep_seat() => (),
                }
            }
            log::info!("watcher disconnected");
        });

        Ok(Self {
            inner: HlsLiveSource::new(client, stream.uri, None, None),
            retry: 3,
        })
    }

    pub fn with_retry(mut self, retry: u32) -> Self {
        self.retry = retry;
        self
    }
}

impl StreamingSource for NicoTimeshiftSource {
    type Segment = M3u8Segment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        self.inner.fetch_info().await
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        self.inner.fetch_segment(segment, writer).await
    }
}
