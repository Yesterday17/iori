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

use super::{segment::M3u8Segment, utils::load_m3u8};
use crate::{decrypt::IoriKey, error::IoriResult};

/// Core part to perform network operations
pub struct M3u8Source {
    m3u8_url: String,

    key: Option<String>,
    shaka_packager_command: Option<PathBuf>,

    sequence: AtomicU64,
    client: Arc<Client>,
}

impl M3u8Source {
    pub fn new(
        client: Arc<Client>,
        m3u8_url: String,
        key: Option<String>,
        shaka_packager_command: Option<PathBuf>,
    ) -> Self {
        Self {
            m3u8_url,
            key,
            shaka_packager_command,

            sequence: AtomicU64::new(0),
            client,
        }
    }

    // TODO: return an iterator instead of a Vec
    pub async fn load_segments(
        &self,
        latest_media_sequence: Option<u64>,
    ) -> IoriResult<(Vec<M3u8Segment>, Url, MediaPlaylist)> {
        let (playlist_url, playlist) =
            load_m3u8(&self.client, Url::from_str(&self.m3u8_url)?).await?;

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