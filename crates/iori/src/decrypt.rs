use std::{
    collections::HashMap,
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    process::Command,
};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use m3u8_rs::KeyMethod;

use crate::error::{IoriError, IoriResult};

pub enum IoriKey {
    Aes128 {
        key: [u8; 16],
        iv: [u8; 16],
    },
    ClearKey {
        keys: HashMap<String, String>,
        /// Whether to use shaka_packager for decryption
        shaka_packager_command: Option<PathBuf>,
    },
}

impl IoriKey {
    pub fn clear_key(key: String) -> IoriResult<Self> {
        let (kid, key) = key
            .split_once(':')
            .ok_or_else(|| IoriError::InvalidClearKey(key.clone()))?;

        let mut keys = HashMap::new();
        keys.insert(kid.to_string(), key.to_string());
        Ok(Self::ClearKey {
            keys,
            shaka_packager_command: None,
        })
    }

    pub async fn from_key(
        client: &reqwest::Client,
        key: &m3u8_rs::Key,
        playlist_url: &reqwest::Url,
        media_sequence: u64,
        manual_key: Option<String>,
        shaka_packager_command: Option<PathBuf>,
    ) -> IoriResult<Option<Self>> {
        Ok(match &key.method {
            KeyMethod::None => None,
            KeyMethod::AES128 => {
                let key_bytes = if let Some(key) = manual_key {
                    hex::decode(key)?
                } else {
                    client
                        .get(
                            playlist_url
                                .join(&key.uri.clone().expect("URI field in key must exist"))?,
                        )
                        .send()
                        .await?
                        .bytes()
                        .await?
                        .to_vec()
                };
                Some(Self::Aes128 {
                    key: key_bytes
                        .try_into()
                        .map_err(|v| IoriError::InvalidAes128Key(v))?,
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
                    log::debug!("{name} encryption detected. Using manual key.");

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

                    Some(Self::ClearKey {
                        keys,
                        shaka_packager_command,
                    })
                }
                _ => unimplemented!("Unknown key method: {name}"),
            },
        })
    }

    pub fn to_decryptor(&self) -> IoriDecryptor {
        match self {
            IoriKey::Aes128 { key, iv } => {
                IoriDecryptor::Aes128(cbc::Decryptor::<aes::Aes128>::new(key.into(), iv.into()))
            }
            IoriKey::ClearKey {
                keys,
                shaka_packager_command,
            } => IoriDecryptor::Mp4Decrypt {
                keys: keys.clone(),
                shaka_packager_command: shaka_packager_command.clone(),
            },
        }
    }
}

pub enum IoriDecryptor {
    Aes128(cbc::Decryptor<aes::Aes128>),
    Mp4Decrypt {
        keys: HashMap<String, String>,
        shaka_packager_command: Option<PathBuf>,
    },
}

impl IoriDecryptor {
    pub fn decrypt(self, data: &[u8]) -> IoriResult<Vec<u8>> {
        Ok(match self {
            IoriDecryptor::Aes128(decryptor) => decryptor.decrypt_padded_vec_mut::<Pkcs7>(&data)?,
            IoriDecryptor::Mp4Decrypt {
                keys,
                shaka_packager_command,
            } => {
                if let Some(shaka_packager_command) = shaka_packager_command {
                    let temp_dir = tempfile::tempdir()?;
                    let rand_suffix = rand::random::<u64>();
                    let temp_input_file = temp_dir.path().join(format!("input_{rand_suffix}.mp4"));
                    let temp_output_file =
                        temp_dir.path().join(format!("output_{rand_suffix}.mp4"));

                    let mut input = File::create(&temp_input_file)?;
                    input.write_all(data)?;

                    let mut command = Command::new(shaka_packager_command);
                    command
                        .arg("--quiet")
                        .arg("--enable_raw_key_decryption")
                        .arg({
                            let mut str = OsString::new();
                            str.push("input=");
                            str.push(temp_input_file.as_os_str());
                            str.push(",stream=0,output=");
                            str.push(temp_output_file.as_os_str());
                            str
                        });

                    for (kid, key) in keys {
                        command
                            .arg("--keys")
                            .arg(format!("key_id={}:key={}", kid, key));
                    }
                    command.spawn()?.wait()?;

                    let mut file = File::open(temp_output_file)?;
                    let mut data = Vec::new();
                    file.read_to_end(&mut data)?;
                    data
                } else {
                    mp4decrypt::mp4decrypt(data, keys, None).map_err(IoriError::Mp4DecryptError)?
                }
            }
        })
    }
}

fn is_valid_kid_key_pair(kid: &str, key: &str) -> bool {
    kid.len() == 32
        && key.len() == 32
        && kid.chars().all(|c| c.is_ascii_hexdigit())
        && key.chars().all(|c| c.is_ascii_hexdigit())
}
