use std::{
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use m3u8_rs::MediaPlaylist;
use reqwest::{Client, Url};

use crate::{
    decrypt::IoriKey,
    error::IoriResult,
    hls::{segment::M3u8Segment, utils::load_m3u8},
    IoriError,
};

/// Core part to perform network operations
pub struct M3u8Source {
    m3u8_url: String,

    key: Option<String>,
    shaka_packager_command: Option<PathBuf>,

    initial_playlist: Option<String>,

    sequence: AtomicU64,
    client: Client,
}

impl M3u8Source {
    pub fn new(
        client: Client,
        m3u8_url: String,
        initial_playlist: Option<String>,
        key: Option<&str>,
        shaka_packager_command: Option<PathBuf>,
    ) -> Self {
        Self {
            m3u8_url,
            initial_playlist,
            key: key.map(str::to_string),
            shaka_packager_command,

            sequence: AtomicU64::new(0),
            client,
        }
    }

    pub async fn load_segments(
        &mut self,
        latest_media_sequence: Option<u64>,
        retry: u32,
    ) -> IoriResult<(Vec<M3u8Segment>, Url, MediaPlaylist)> {
        let (playlist_url, playlist) = if let Some(initial_playlist) = self.initial_playlist.take()
        {
            let parsed_playlist = m3u8_rs::parse_media_playlist_res(initial_playlist.as_bytes());
            match parsed_playlist {
                Ok(parsed_playlist) => (Url::from_str(&self.m3u8_url)?, parsed_playlist),
                Err(_) => return Err(IoriError::M3u8ParseError(initial_playlist)),
            }
        } else {
            load_m3u8(&self.client, Url::from_str(&self.m3u8_url)?, retry).await?
        };

        let mut key = None;
        let mut initial_block = None;
        let mut segments = Vec::with_capacity(playlist.segments.len());
        for (i, segment) in playlist.segments.iter().enumerate() {
            if let Some(k) = &segment.key {
                key = IoriKey::from_key(
                    &self.client,
                    k,
                    &playlist_url,
                    playlist.media_sequence,
                    self.key.clone(),
                    self.shaka_packager_command.clone(),
                )
                .await?
                .map(Arc::new);
            }

            if let Some(m) = &segment.map {
                let url = playlist_url.join(&m.uri)?;
                let bytes = self.client.get(url).send().await?.bytes().await?.to_vec();
                initial_block = Some(Arc::new(bytes));
            }

            let url = playlist_url.join(&segment.uri)?;
            // FIXME: filename may be too long
            let filename = url
                .path_segments()
                .and_then(|c| c.last())
                .map(|r| {
                    if r.ends_with(".m4s") {
                        // xx.m4s -> x.mp4
                        format!("{}.mp4", &r[..r.len() - 4])
                    } else {
                        r.to_string()
                    }
                })
                .unwrap_or("output.ts".to_string());

            let media_sequence = playlist.media_sequence + i as u64;
            if let Some(latest_media_sequence) = latest_media_sequence {
                if media_sequence <= latest_media_sequence as u64 {
                    continue;
                }
            }

            let segment = M3u8Segment {
                url,
                filename,
                key: key.clone(),
                initial_segment: initial_block.clone(),
                sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
                media_sequence,
                byte_range: segment.byte_range.clone(),
            };
            segments.push(segment);
        }

        Ok((segments, playlist_url, playlist))
    }
}
