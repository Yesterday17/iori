use std::{
    hash::{Hash, Hasher},
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use m3u8_rs::{AlternativeMedia, AlternativeMediaType, MediaPlaylist, Playlist};
use reqwest::Url;

use crate::{
    decrypt::IoriKey,
    error::IoriResult,
    hls::{segment::M3u8Segment, utils::load_m3u8},
    util::http::HttpClient,
    InitialSegment, SegmentFormat, SegmentType,
};

use super::utils::load_playlist_with_retry;

/// Core part to perform network operations
pub struct M3u8Source {
    m3u8_url: String,

    key: Option<String>,

    initial_playlist: Option<MediaPlaylist>,

    stream_id: u64,
    sequence: AtomicU64,
    client: HttpClient,
    segment_type: Option<SegmentType>,

    retry: u32,
}

impl M3u8Source {
    pub fn new(
        client: HttpClient,
        m3u8_url: String,
        initial_playlist: Option<MediaPlaylist>,
        key: Option<&str>,
        segment_type: Option<SegmentType>,
        stream_id: u64,
        retry: u32,
    ) -> Self {
        Self {
            m3u8_url,
            initial_playlist,
            key: key.map(str::to_string),

            sequence: AtomicU64::new(0),
            client,
            segment_type,
            stream_id,

            retry,
        }
    }

    pub async fn load_segments(
        &mut self,
        latest_media_sequence: &Option<u64>,
        retry: u32,
    ) -> IoriResult<(Vec<M3u8Segment>, Url, MediaPlaylist)> {
        let (playlist_url, playlist) = if let Some(initial_playlist) = self.initial_playlist.take()
        {
            (Url::from_str(&self.m3u8_url)?, initial_playlist)
        } else {
            load_m3u8(&self.client, Url::from_str(&self.m3u8_url)?, retry).await?
        };

        let mut key = None;
        let mut initial_segment = InitialSegment::None;
        let mut segments = Vec::with_capacity(playlist.segments.len());
        for (i, segment) in playlist.segments.iter().enumerate() {
            if let Some(k) = &segment.key {
                key = IoriKey::from_key(
                    &self.client,
                    k,
                    &playlist_url,
                    playlist.media_sequence,
                    self.key.clone(),
                )
                .await?
                .map(Arc::new);
            }

            if let Some(m) = &segment.map {
                let url = playlist_url.join(&m.uri)?;

                let mut retries = self.retry;
                loop {
                    retries -= 1;

                    match self.load_bytes(url.clone()).await {
                        Ok(bytes) => {
                            initial_segment = if m.after_key {
                                InitialSegment::Encrypted(Arc::new(bytes))
                            } else {
                                InitialSegment::Clear(Arc::new(bytes))
                            };
                            break;
                        }
                        Err(e) => {
                            log::warn!("Failed to load bytes for initial segment {url}: {e}");
                            if retries == 0 {
                                return Err(e);
                            }
                        }
                    }
                }
            }

            let url = playlist_url.join(&segment.uri)?;
            // FIXME: filename may be too long
            let filename = url
                .path_segments()
                .and_then(|c| c.last())
                .map(|r| r.to_string())
                .unwrap_or_else(|| {
                    // 1. hash of file url
                    let mut hasher = std::hash::DefaultHasher::new();
                    url.hash(&mut hasher);
                    let value = hasher.finish();
                    let mut filename = format!("{value:016x}");

                    // 2. byte range
                    if let Some(byte_range) = &segment.byte_range {
                        filename.push_str(&format!("_{}", byte_range.length));
                        if let Some(offset) = byte_range.offset {
                            filename.push_str(&format!("_{}", offset));
                        }
                    }

                    filename
                });
            let format = SegmentFormat::from_filename(&filename);

            let media_sequence = playlist.media_sequence + i as u64;
            if let Some(latest_media_sequence) = latest_media_sequence {
                if media_sequence <= *latest_media_sequence as u64 {
                    continue;
                }
            }

            let segment = M3u8Segment {
                stream_id: self.stream_id,
                url,
                filename,
                key: key.clone(),
                initial_segment: initial_segment.clone(),
                sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
                media_sequence,
                byte_range: segment.byte_range.clone(),
                duration: segment.duration,
                segment_type: self.segment_type.clone(),
                format,
            };
            segments.push(segment);
        }

        Ok((segments, playlist_url, playlist))
    }

    async fn load_bytes(&self, url: Url) -> IoriResult<Vec<u8>> {
        Ok(self.client.get(url).send().await?.bytes().await?.to_vec())
    }
}

pub struct AdvancedM3u8Source {
    m3u8_url: Url,

    streams: Vec<M3u8Source>,

    key: Option<String>,
    client: HttpClient,

    retry: u32,
}

impl AdvancedM3u8Source {
    pub fn new(client: HttpClient, m3u8_url: Url, key: Option<&str>, retry: u32) -> Self {
        Self {
            m3u8_url,
            key: key.map(str::to_string),
            client,
            streams: Vec::new(),

            retry,
        }
    }

    pub async fn load_streams(&mut self, retry: u32) -> IoriResult<Vec<Option<u64>>> {
        let playlist = load_playlist_with_retry(&self.client, &self.m3u8_url, retry).await?;

        match playlist {
            Playlist::MasterPlaylist(mut pl) => {
                // Get the best variant
                let variants = &mut pl.variants;
                variants.sort_by(|a, b| {
                    // compare resolution first
                    if let (Some(a), Some(b)) = (a.resolution, b.resolution) {
                        if a.width != b.width {
                            return b.width.cmp(&a.width);
                        }
                    }

                    // compare framerate then
                    if let (Some(a), Some(b)) = (a.frame_rate, b.frame_rate) {
                        let a = a as u64;
                        let b = b as u64;
                        if a != b {
                            return b.cmp(&a);
                        }
                    }

                    // compare bandwidth finally
                    b.bandwidth.cmp(&a.bandwidth)
                });
                let variant = variants.get(0).expect("No variant found");
                let variant_url = self
                    .m3u8_url
                    .join(&variant.uri)
                    .expect("Invalid variant uri");
                self.streams.push(M3u8Source::new(
                    self.client.clone(),
                    variant_url.to_string(),
                    None,
                    self.key.as_deref(),
                    Some(SegmentType::Video),
                    0,
                    self.retry,
                ));

                fn load_variant<'a, 'b>(
                    group_id: &'a str,
                    media_type: AlternativeMediaType,
                    pl: &'b Vec<AlternativeMedia>,
                ) -> Option<&'b str> {
                    let alternatives: Vec<_> = pl
                        .iter()
                        .filter(|alternative| {
                            alternative.group_id == group_id && alternative.media_type == media_type
                        })
                        .collect();

                    let best = alternatives
                        .iter()
                        .find(|alternative| alternative.default && alternative.autoselect)
                        .or_else(|| alternatives.first());

                    best.and_then(|b| b.uri.as_deref())
                }

                // Load extra streams from the variant
                if let Some(group_id) = &variant.audio {
                    if let Some(audio_url) =
                        load_variant(&group_id, AlternativeMediaType::Audio, &pl.alternatives)
                    {
                        self.streams.push(M3u8Source::new(
                            self.client.clone(),
                            self.m3u8_url
                                .join(audio_url)
                                .expect("Invalid audio uri")
                                .to_string(),
                            None,
                            self.key.as_deref(),
                            Some(SegmentType::Audio),
                            1,
                            self.retry,
                        ));
                    }
                }
                if let Some(group_id) = &variant.video {
                    if let Some(video_url) =
                        load_variant(&group_id, AlternativeMediaType::Video, &pl.alternatives)
                    {
                        self.streams.push(M3u8Source::new(
                            self.client.clone(),
                            video_url.to_string(),
                            None,
                            self.key.as_deref(),
                            Some(SegmentType::Video),
                            2,
                            self.retry,
                        ));
                    }
                }
            }
            Playlist::MediaPlaylist(pl) => {
                self.streams.push(M3u8Source::new(
                    self.client.clone(),
                    self.m3u8_url.to_string(),
                    Some(pl),
                    self.key.as_deref(),
                    Some(SegmentType::Video),
                    0,
                    self.retry,
                ));
            }
        }
        Ok(vec![None; self.streams.len()])
    }

    pub async fn load_segments(
        &mut self,
        latest_media_sequences: &[Option<u64>],
        retry: u32,
    ) -> IoriResult<(Vec<Vec<M3u8Segment>>, bool /* is_end */)> {
        let mut segments = Vec::new();
        let mut is_end = true;
        for (stream, latest_media_sequence) in self.streams.iter_mut().zip(latest_media_sequences) {
            let (stream_segments, _, stream_playlist) =
                stream.load_segments(latest_media_sequence, retry).await?;
            segments.push(stream_segments);
            if !stream_playlist.end_list {
                is_end = false;
            }
        }

        Ok((segments, is_end))
    }
}
