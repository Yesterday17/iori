use std::{
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use m3u8_rs::KeyMethod;
use reqwest::{Client, Url};
use tokio::{fs::File, io::AsyncWriteExt, sync::mpsc};

use super::{utils::load_m3u8, M3u8Aes128Key, M3u8Segment};
use crate::{StreamingDownloaderExt, StreamingSource};

pub struct CommonM3u8ArchiveDownloader {
    m3u8_url: String,

    output_dir: PathBuf,
    sequence: AtomicU64,
    client: Arc<Client>,
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
}

impl StreamingSource for CommonM3u8ArchiveDownloader {
    type Segment = M3u8Segment;

    async fn fetch_info(&mut self) -> mpsc::UnboundedReceiver<Self::Segment> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let (playlist_url, playlist) =
            load_m3u8(&self.client, Url::from_str(&self.m3u8_url).unwrap()).await;

        let mut key = None;
        for segment in playlist.segments {
            if let Some(k) = segment.key {
                let new_key = match k.method {
                    KeyMethod::None => None,
                    KeyMethod::AES128 => {
                        let key = self
                            .client
                            .get(playlist_url.join(&k.uri.unwrap()).unwrap())
                            .send()
                            .await
                            .unwrap()
                            .bytes()
                            .await
                            .unwrap();
                        Some(M3u8Aes128Key {
                            key: key.to_vec().try_into().unwrap(),
                            iv: k
                                .iv
                                .and_then(|iv| {
                                    let iv = if iv.starts_with("0x") {
                                        &iv[2..]
                                    } else {
                                        iv.as_str()
                                    };
                                    u128::from_str_radix(iv, 16).ok()
                                })
                                .unwrap_or_else(|| playlist.media_sequence as u128)
                                .to_be_bytes(),
                            keyformat: k.keyformat,
                            keyformatversions: k.keyformatversions,
                        })
                    }
                    KeyMethod::SampleAES => todo!(),
                    KeyMethod::Other(_) => unimplemented!(),
                };
                key = new_key;
            }

            let url = playlist_url.join(&segment.uri).unwrap();
            // FIXME: filename may be too long
            let filename = url
                .path_segments()
                .and_then(|c| c.last())
                .unwrap_or("output.ts")
                .to_string();

            let segment = M3u8Segment {
                url,
                filename,
                key: key.clone(),
                sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
            };
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
        let decryptor = segment
            .key
            .map(|key| cbc::Decryptor::<aes::Aes128>::new(&key.key.into(), &key.iv.into()));
        if let Some(decryptor) = decryptor {
            let bytes = decryptor.decrypt_padded_vec_mut::<Pkcs7>(&bytes).unwrap();
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
