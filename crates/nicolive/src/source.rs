use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, LazyLock,
};

use iori::{
    fetch::fetch_segment, hls::utils::load_m3u8, IoriResult, RemoteStreamingSegment, SegmentType,
    StreamingSegment, StreamingSource,
};
use parking_lot::RwLock;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncWrite,
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
};
use url::Url;

use crate::model::WatchResponse;

pub struct NicoTimeshiftSegment {
    host: Arc<RwLock<Url>>,
    token: Arc<RwLock<String>>,

    /// A semaphore permit to inform the source that the segment has been fetched
    _permit: OwnedSemaphorePermit,

    ts: String,
    file_name: String,
    query: Option<String>,
    /// Sequence id allocated by the downloader, starts from 0
    sequence: u64,
}

impl StreamingSegment for NicoTimeshiftSegment {
    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }

    fn key(&self) -> Option<Arc<iori::decrypt::IoriKey>> {
        None
    }

    fn r#type(&self) -> SegmentType {
        SegmentType::Video
    }
}

impl RemoteStreamingSegment for NicoTimeshiftSegment {
    fn url(&self) -> reqwest::Url {
        let host = self.host.read().clone();
        let token = self.token.read().clone();

        let mut url = host
            .join(&format!("{}/{}", self.ts, self.file_name))
            .unwrap();
        url.set_query(self.query.as_deref());

        // remove ht2_nicolive first
        let query: Vec<(_, _)> = url
            .query_pairs()
            .filter(|(name, _)| name != "ht2_nicolive")
            .map(|r| (r.0.to_string(), r.1.to_string()))
            .collect();
        // add new ht2_nicolive token then
        url.query_pairs_mut()
            .clear()
            .extend_pairs(query)
            .append_pair("ht2_nicolive", token.as_str());

        url
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NicoTimeshiftSegmentInfo {
    sequence: u64,
    file_name: String,
}

pub struct NicoTimeshiftSource {
    client: Client,

    m3u8_url: String,
    sequence: Arc<AtomicU64>,

    host: Arc<RwLock<Url>>,
    token: Arc<RwLock<String>>,
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
        let token = url
            .query_pairs()
            .find(|(key, _)| key == "ht2_nicolive")
            .map(|r| r.1)
            .expect("No ht2_nicolive token found")
            .to_string();
        let host = Arc::new(RwLock::new(url));
        let token = Arc::new(RwLock::new(token));

        let host_cloned = host.clone();
        let token_cloned = token.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                watcher.init().await.unwrap();

                let stream = loop {
                    let msg = watcher.recv().await;
                    if let Ok(Some(WatchResponse::Stream(stream))) = msg {
                        break stream;
                    }
                };

                let url = Url::parse(&stream.uri).unwrap();
                let token = url
                    .query_pairs()
                    .find(|(key, _)| key == "ht2_nicolive")
                    .map(|r| r.1)
                    .expect("No ht2_nicolive token found")
                    .to_string();
                log::info!("Update Token: {token}");
                *host_cloned.write() = url;
                *token_cloned.write() = token;
            }
        });

        Ok(Self {
            client: client.clone(),
            m3u8_url: stream.uri,
            sequence: Arc::new(AtomicU64::new(0)),
            host,
            token,
            retry: 3,
        })
    }

    pub fn with_retry(mut self, retry: u32) -> Self {
        self.retry = retry;
        self
    }
}

const NICO_SEGMENT_OFFSET_REGEXP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(\d{3})\.ts"#).unwrap());

impl StreamingSource for NicoTimeshiftSource {
    type Segment = NicoTimeshiftSegment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let (playlist_url, playlist) =
            load_m3u8(&self.client, Url::parse(&self.m3u8_url)?, self.retry).await?;
        let chunk_length = (playlist.segments.iter().map(|s| s.duration).sum::<f32>()
            / playlist.segments.len() as f32) as u64;

        let playlist_text = self
            .client
            .get(playlist_url.clone())
            .send()
            .await?
            .text()
            .await?;
        let regex = Regex::new(r#"#DMC-STREAM-DURATION:(.+)"#).unwrap();
        let video_length = regex
            .captures(&playlist_text)
            .and_then(|cap| cap.get(1))
            .and_then(|d| d.as_str().parse().ok())
            .ok_or_else(|| anyhow::anyhow!("{playlist_text}"))
            .expect("Failed to parse video length");

        log::info!("video_length: {video_length}, chunk_length: {chunk_length}");
        // let video_length: f32 = playlist
        //     .unknown_tags
        //     .into_iter()
        //     .find(|r| r.tag == "DMC-STREAM-DURATION")
        //     .and_then(|t| t.rest)
        //     .and_then(|d| d.parse().ok())
        //     .unwrap();

        let first_chunk_url = &playlist.segments[0].uri;
        let second_chunk_url = &playlist.segments[1].uri;
        let offset = NICO_SEGMENT_OFFSET_REGEXP
            .captures(if first_chunk_url.starts_with("0.ts") {
                second_chunk_url
            } else {
                first_chunk_url
            })
            .unwrap()
            .get(1)
            .unwrap()
            .as_str()
            .to_string();
        log::debug!("offset: {offset}");

        let limit = playlist.segments.len();

        let sequence = self.sequence.clone();
        let client = self.client.clone();

        let host = self.host.clone();
        let token = self.token.clone();
        tokio::spawn(async move {
            let permits = Arc::new(Semaphore::new(limit));

            let mut time = 0.;

            while time < video_length {
                if video_length - format!("{time}.{offset}").parse::<f32>().unwrap() < 1. {
                    break;
                }

                let mut url = playlist_url.clone();
                // replace `start` with the current time
                let query: Vec<(_, _)> = playlist_url
                    .query_pairs()
                    .filter(|(name, _)| name != "start")
                    .collect();
                url.query_pairs_mut()
                    .clear()
                    .extend_pairs(query)
                    .append_pair("start", &format!("{time}"));
                log::debug!("ping {url}");

                // fetch url(ping), ignore the result
                let _ = client.get(url.clone()).send().await;

                // https://liveedge265.dmc.nico/hlsarchive/ht2_nicolive/nicolive-production-pg41793477411455_4a94f2f2a857a6bf7dca13d2825bf5acef5c8c77fedf0dd83912367632a4c7b1/1/ts/playlist.m3u8?start_time=-575435206444&ht2_nicolive=86127604.knv7k8rg2e_sa5alt_3rt0vxccmbc1b&start=15.114
                // Extract the 1/ts part
                let regex = Regex::new(r#"(?:http(?:s):\/\/.+\/)(\d\/ts)"#).unwrap();
                let ts = regex
                    .captures(&url.to_string())
                    .and_then(|cap| cap.get(1))
                    .map(|r| r.as_str().to_string())
                    .unwrap();

                // 0-<limit>, <limit> chunks per list
                // fetch the next <limit> chunks
                let mut segments = Vec::new();
                for _ in 0..limit {
                    let permit = permits.clone().acquire_owned().await.unwrap();
                    let filename = if time == 0. {
                        format!("0.ts")
                    } else {
                        format!("{time}{offset}.ts")
                    };
                    let mut segment_url = url.join(&filename).unwrap();
                    segment_url.set_query(url.query());

                    segments.push(NicoTimeshiftSegment {
                        host: host.clone(),
                        token: token.clone(),
                        _permit: permit,

                        ts: ts.clone(),
                        file_name: filename,
                        query: url.query().map(|q| q.to_string()),
                        sequence: sequence.fetch_add(1, Ordering::Relaxed),
                    });

                    time += chunk_length as f32;
                    if video_length - format!("{time}.{offset}").parse::<f32>().unwrap() < 1. {
                        break;
                    }
                }

                // send segments
                if let Err(_) = sender.send(Ok(segments)) {
                    break;
                }

                // wait for all segments to be fetched
                let _ = permits.acquire_many(limit as u32).await;
            }
        });
        Ok(receiver)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        fetch_segment(self.client.clone(), segment, writer).await?;
        Ok(())
    }
}
