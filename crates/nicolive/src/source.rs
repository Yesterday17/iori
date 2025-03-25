use iori::{
    hls::{segment::M3u8Segment, CommonM3u8LiveSource},
    IoriResult, StreamingSource,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWrite, sync::mpsc};
use url::Url;

use crate::model::{StreamCookies, WatchResponse};

#[derive(Debug, Serialize, Deserialize)]
pub struct NicoTimeshiftSegmentInfo {
    sequence: u64,
    file_name: String,
}

pub struct NicoTimeshiftSource {
    m3u8_url: Url,
    inner: CommonM3u8LiveSource,

    cookies: StreamCookies,
    retry: u32,
}

impl NicoTimeshiftSource {
    pub async fn new(client: Client, wss_url: String) -> anyhow::Result<Self> {
        let mut watcher = crate::watch::WatchClient::new(&wss_url).await?;
        watcher.init().await?;

        let stream = loop {
            let msg = watcher.recv().await?;
            if let Some(WatchResponse::Stream(stream)) = msg {
                break stream;
            }
        };

        let url = Url::parse(&stream.uri)?;
        let cookies = stream.cookies;

        tokio::spawn(async move {
            loop {
                _ = watcher.recv().await;
            }
        });

        log::info!("Playlist: {url}");

        Ok(Self {
            m3u8_url: url,
            inner: CommonM3u8LiveSource::new(client, stream.uri, None, None),
            cookies,
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
        let (sender, receiver) = mpsc::unbounded_channel();

        let headers = self.cookies.to_headers(&self.m3u8_url.path());
        let mut inner_receiver = self.inner.fetch_info().await?;
        tokio::spawn(async move {
            while let Some(Ok(mut segments)) = inner_receiver.recv().await {
                if let Some(headers) = &headers {
                    for segment in segments.iter_mut() {
                        segment.headers = Some(headers.clone());
                    }
                }

                sender.send(Ok(segments)).unwrap();
            }
        });

        Ok(receiver)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        self.inner.fetch_segment(segment, writer).await
    }
}
