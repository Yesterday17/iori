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
use crate::{StreamingDownloaderExt, StreamingSource};

pub struct CommonM3u8ArchiveDownloader {
    pub(crate) m3u8_url: String,

    pub(crate) output_dir: PathBuf,
    pub(crate) sequence: AtomicU64,
    pub(crate) client: Arc<Client>,
}

impl CommonM3u8ArchiveDownloader {
    pub fn new(m3u8: String, output_dir: PathBuf) -> Self {
        let client = Arc::new(Client::new());
        Self {
            m3u8_url: m3u8,
            output_dir,

            sequence: AtomicU64::new(0),
            client,
        }
    }

    // TODO: return an iterator instead of a Vec
    pub(crate) async fn load_segments(
        &self,
        latest_media_sequence: Option<u64>,
    ) -> (Vec<M3u8Segment>, Url, MediaPlaylist) {
        let (playlist_url, playlist) =
            load_m3u8(&self.client, Url::from_str(&self.m3u8_url).unwrap()).await;

        let mut key = None;
        let mut segments = Vec::with_capacity(playlist.segments.len());
        for (i, segment) in playlist.segments.iter().enumerate() {
            if let Some(k) = &segment.key {
                key = M3u8Key::from_key(&self.client, k, &playlist_url, playlist.media_sequence)
                    .await;
            }

            let url = playlist_url.join(&segment.uri).unwrap();
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
                sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
                media_sequence,
            };
            segments.push(segment);
        }

        (segments, playlist_url, playlist)
    }
}

impl StreamingSource for CommonM3u8ArchiveDownloader {
    type Segment = M3u8Segment;

    async fn fetch_info(&mut self) -> mpsc::UnboundedReceiver<Self::Segment> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let (segments, _, _) = self.load_segments(None).await;
        for segment in segments {
            if let Err(_) = sender.send(segment) {
                break;
            }
        }
        receiver
    }

    async fn fetch_segment(&self, segment: Self::Segment) {
        if !self.output_dir.exists() {
            tokio::fs::create_dir_all(&self.output_dir).await.unwrap();
        }

        let filename = segment.filename;
        let sequence = segment.sequence;
        let mut tmp_file = File::create(self.output_dir.join(format!("{sequence:06}_{filename}")))
            .await
            .unwrap();

        let bytes = self
            .client
            .get(segment.url)
            .send()
            .await
            .expect("http error")
            .bytes()
            .await
            .unwrap();
        // TODO: use bytes_stream to improve performance
        // .bytes_stream();
        let decryptor = segment.key.map(|key| key.to_decryptor());
        if let Some(decryptor) = decryptor {
            let bytes = decryptor.decrypt(&bytes);
            tmp_file.write_all(&bytes).await.unwrap();
        } else {
            tmp_file.write_all(&bytes).await.unwrap();
        }

        // let mut buf = EagerBuffer::<block_buffer::generic_array::typenum::consts::U16>::default();
        // while let Some(item) = byte_stream.next().await {
        //     let input = item.unwrap();
        //     let mut input = input.to_vec();
        //     if let Some(decryptor) = decryptor.as_mut() {
        //         buf.set_data(&mut input, |blocks| {
        //             if blocks.is_empty() {
        //                 return;
        //             }

        //             decryptor.decrypt_blocks_mut(blocks);
        //             result.push(blocks.to_vec());
        //         });
        //     } else {
        //         tmp_file.write_all(&mut input).await.unwrap();
        //     }
        // }
    }
}

impl StreamingDownloaderExt for CommonM3u8ArchiveDownloader {}

#[cfg(test)]
mod tests {
    use crate::StreamingDownloaderExt;

    use super::*;

    #[tokio::test]
    async fn test_download_archive() {
        let mut downloader = CommonM3u8ArchiveDownloader::new(
            "https://test-streams.mux.dev/bbbAES/playlists/sample_aes/index.m3u8".to_string(),
            "/tmp/test".into(),
        );
        downloader.download().await;
    }
}
