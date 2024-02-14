use std::collections::HashMap;

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use data_url::DataUrl;
use m3u8_rs::KeyMethod;

pub enum M3u8Key {
    Aes128 {
        key: [u8; 16],
        iv: [u8; 16],
    },
    Mp4Decrypt {
        keys: HashMap<String, String>,
        pssh: Vec<u8>,
    },
}

impl M3u8Key {
    pub async fn from_key(
        client: &reqwest::Client,
        key: &m3u8_rs::Key,
        playlist_url: &reqwest::Url,
        media_sequence: u64,
        manual_key: Option<String>,
    ) -> Option<Self> {
        match &key.method {
            KeyMethod::None => None,
            KeyMethod::AES128 => {
                let key_bytes = if let Some(key) = manual_key {
                    hex::decode(key).unwrap()
                } else {
                    client
                        .get(playlist_url.join(&key.uri.clone().unwrap()).unwrap())
                        .send()
                        .await
                        .unwrap()
                        .bytes()
                        .await
                        .unwrap()
                        .to_vec()
                };
                Some(Self::Aes128 {
                    key: key_bytes.try_into().unwrap(),
                    iv: key
                        .iv
                        .clone()
                        .and_then(|iv| {
                            let iv = if iv.starts_with("0x") {
                                &iv[2..]
                            } else {
                                iv.as_str()
                            };
                            u128::from_str_radix(iv, 16).ok()
                        })
                        .unwrap_or(media_sequence as u128)
                        .to_be_bytes(),
                })
            }
            KeyMethod::SampleAES => todo!(),
            KeyMethod::Other(name) => match name.as_str() {
                "SAMPLE-AES-CENC" | "SAMPLE-AES-CTR" => {
                    log::info!("{name} encryption detected. Using manual key.");

                    // <kid>:<key>;<kid>:<key>;...
                    let manual_key =
                        manual_key.expect("Specify key for SAMPLE-AES-CENC/CTR is required.");
                    let mut keys = HashMap::new();
                    for pair in manual_key.split(';') {
                        match pair.split_once(':') {
                            Some((kid, key)) if is_valid_kid_key_pair(kid, key) => {
                                keys.insert(kid.to_string(), key.to_string());
                            }
                            _ => log::warn!("Ignored invalid key format: {}", pair),
                        }
                    }
                    if keys.is_empty() {
                        panic!("No valid key found in {}", manual_key);
                    }

                    // https://github.com/shaka-project/shaka-player/blob/140079d1094effa5f8471bc0c47806ff5e351e97/lib/hls/hls_parser.js
                    let url = DataUrl::process(key.uri.as_deref().unwrap()).unwrap();
                    let (pssh, _) = url.decode_to_vec().unwrap();
                    Some(Self::Mp4Decrypt { keys, pssh })
                }
                _ => unimplemented!("Unknown key method: {name}"),
            },
        }
    }

    pub fn to_decryptor(&self) -> M3u8Decryptor {
        match self {
            M3u8Key::Aes128 { key, iv } => {
                M3u8Decryptor::Aes128(cbc::Decryptor::<aes::Aes128>::new(key.into(), iv.into()))
            }
            M3u8Key::Mp4Decrypt { keys, pssh: _ } => {
                M3u8Decryptor::Mp4Decrypt { keys: keys.clone() }
            }
        }
    }
}

pub enum M3u8Decryptor {
    Aes128(cbc::Decryptor<aes::Aes128>),
    Mp4Decrypt { keys: HashMap<String, String> },
}

impl M3u8Decryptor {
    pub fn decrypt(self, data: &[u8]) -> Vec<u8> {
        match self {
            M3u8Decryptor::Aes128(decryptor) => {
                decryptor.decrypt_padded_vec_mut::<Pkcs7>(&data).unwrap()
            }
            M3u8Decryptor::Mp4Decrypt { keys } => mp4decrypt::mp4decrypt(data, keys, None).unwrap(),
        }
    }
}

fn is_valid_kid_key_pair(kid: &str, key: &str) -> bool {
    kid.len() == 32
        && key.len() == 32
        && kid.chars().all(|c| c.is_ascii_hexdigit())
        && key.chars().all(|c| c.is_ascii_hexdigit())
}
