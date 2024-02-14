use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

#[derive(Clone, Debug)]
pub struct M3u8Aes128Key {
    pub key: [u8; 16],
    pub iv: [u8; 16],
    pub keyformat: Option<String>,
    pub keyformatversions: Option<String>,
}

impl M3u8Aes128Key {
    pub async fn from_key(
        client: &reqwest::Client,
        key: m3u8_rs::Key,
        playlist_url: &reqwest::Url,
        media_sequence: u64,
    ) -> Self {
        assert!(matches!(key.method, m3u8_rs::KeyMethod::AES128));

        let key_bytes = client
            .get(playlist_url.join(&key.uri.unwrap()).unwrap())
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        M3u8Aes128Key {
            key: key_bytes.to_vec().try_into().unwrap(),
            iv: key
                .iv
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
            keyformat: key.keyformat,
            keyformatversions: key.keyformatversions,
        }
    }

    pub fn to_decryptor(&self) -> M3u8Aes128Decryptor {
        M3u8Aes128Decryptor(cbc::Decryptor::<aes::Aes128>::new(
            &self.key.into(),
            &self.iv.into(),
        ))
    }
}

pub struct M3u8Aes128Decryptor(cbc::Decryptor<aes::Aes128>);

impl M3u8Aes128Decryptor {
    pub fn decrypt(self, data: &[u8]) -> Vec<u8> {
        self.0.decrypt_padded_vec_mut::<Pkcs7>(&data).unwrap()
    }
}
