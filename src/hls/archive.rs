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
use tokio::{fs::File, io::AsyncWriteExt, sync::mpsc};

use super::{decrypt::M3u8Key, utils::load_m3u8, M3u8Segment};
use crate::{error::IoriResult, StreamingSource};

pub struct CommonM3u8ArchiveSource {
    m3u8_url: String,

    key: Option<String>,
    shaka_packager_command: Option<PathBuf>,

    output_dir: PathBuf,
    sequence: AtomicU64,
    client: Arc<Client>,
}

impl CommonM3u8ArchiveSource {
    pub fn new(
        client: Client,
        m3u8: String,
        key: Option<String>,
        output_dir: PathBuf,
        shaka_packager_command: Option<PathBuf>,
    ) -> Self {
        let client = Arc::new(client);
        Self {
            m3u8_url: m3u8,
            key,
            shaka_packager_command,
            output_dir,

            sequence: AtomicU64::new(0),
            client,
        }
    }

    // TODO: return an iterator instead of a Vec
    pub(crate) async fn load_segments(
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
                key = M3u8Key::from_key(
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
                .unwrap_or("output.ts")
                .to_string();

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
            };
            segments.push(segment);
        }

        Ok((segments, playlist_url, playlist))
    }
}

impl StreamingSource for CommonM3u8ArchiveSource {
    type Segment = M3u8Segment;

    async fn fetch_info(&mut self) -> IoriResult<mpsc::UnboundedReceiver<Vec<Self::Segment>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let (segments, _, _) = self.load_segments(None).await?;
        let _ = sender.send(segments);

        Ok(receiver)
    }

    async fn fetch_segment(&self, segment: &Self::Segment) -> IoriResult<()> {
        if !self.output_dir.exists() {
            tokio::fs::create_dir_all(&self.output_dir).await?;
        }

        let filename = &segment.filename;
        let sequence = segment.sequence;
        let mut tmp_file =
            File::create(self.output_dir.join(format!("{sequence:06}_{filename}"))).await?;

        let bytes = self
            .client
            .get(segment.url.clone())
            .send()
            .await?
            .bytes()
            .await?;
        // TODO: use bytes_stream to improve performance
        // .bytes_stream();
        let decryptor = segment.key.clone().map(|key| key.to_decryptor());
        if let Some(decryptor) = decryptor {
            let bytes = if let Some(initial_segment) = &segment.initial_segment {
                let mut result = initial_segment.to_vec();
                result.extend_from_slice(&bytes);
                result
            } else {
                bytes.to_vec()
            };
            let bytes = decryptor.decrypt(&bytes)?;
            tmp_file.write_all(&bytes).await?;
        } else {
            if let Some(initial_segment) = &segment.initial_segment {
                tmp_file.write_all(initial_segment).await?;
            }
            tmp_file.write_all(&bytes).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::SequencialDownloader;

    #[tokio::test]
    async fn test_download_archive() -> IoriResult<()> {
        let source = CommonM3u8ArchiveSource::new(
            Default::default(),
            "https://test-streams.mux.dev/bbbAES/playlists/sample_aes/index.m3u8".to_string(),
            None,
            "/tmp/test".into(),
            None,
        );
        SequencialDownloader::new(source).download().await?;

        Ok(())
    }
}
