use super::constants::{WIDEVINE_SCHEME_ID_URI, WIDEVINE_SYSTEM_ID};
use crate::base64::base64_decode;
use crate::protocol::WidevinePsshData;
use anyhow::bail;
use byteorder::{BigEndian, ReadBytesExt};
use bytes::Bytes;
use prost::Message;
use std::borrow::Cow;
use std::io::Write;
use std::{
    io::{self, Cursor, Read},
    ops::Deref,
};

pub struct PSSHBox {
    pub kids: Vec<[u8; 16]>,
    pub(crate) data: Option<Vec<u8>>,
}

impl TryFrom<&[u8]> for PSSHBox {
    type Error = io::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if &value[4..8] != b"pssh" {
            return Err(io::Error::new(io::ErrorKind::Other, "Invalid pssh header"));
        }

        let mut buf = Cursor::new(value);
        buf.set_position(20);

        let kid_count = buf.read_u32::<BigEndian>()?;
        let mut kids = Vec::with_capacity(kid_count as usize);
        for _ in 0..kid_count {
            let mut kid = [0u8; 16];
            buf.read_exact(&mut kid)?;
            kids.push(kid);
        }

        let data_length = buf.read_u32::<BigEndian>()?;
        if data_length == 0 {
            return Ok(Self { kids, data: None });
        }

        let mut data = Vec::with_capacity(data_length as usize);
        buf.read_exact(&mut data)?;
        Ok(Self {
            kids,
            data: Some(data),
        })
    }
}

pub struct WidevineInitData(pub(crate) Vec<u8>);

impl WidevineInitData {
    #[doc(hidden)]
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    #[cfg(feature = "mpd")]
    pub fn from_mpd_str<S>(input: S) -> anyhow::Result<Self>
    where
        S: AsRef<str>,
    {
        let mpd = dash_mpd::parse(input.as_ref())?;
        let result: Self = mpd.try_into()?;
        Ok(result)
    }

    pub fn get_pssh(&self) -> anyhow::Result<WidevinePsshData> {
        let init_data = if self.len() < 30 || &self[12..28] == WIDEVINE_SYSTEM_ID {
            Cow::Borrowed(&self.0)
        } else {
            let mut new_pssh = Vec::with_capacity(32 + self.len());
            new_pssh
                .write_all(&[0, 0, 0, 32 + self.len() as u8])
                .unwrap();
            new_pssh.write_all(b"pssh")?;
            new_pssh.write_all(&[0, 0, 0, 0])?;
            new_pssh.write_all(WIDEVINE_SYSTEM_ID)?;
            new_pssh.write_all(&[0, 0, 0, self.len() as u8])?;
            new_pssh.write_all(&self)?;

            Cow::Owned(new_pssh)
        };

        Ok(WidevinePsshData::try_from(init_data.as_slice())?)
    }
}

impl TryFrom<&[u8]> for WidevinePsshData {
    type Error = prost::DecodeError;

    fn try_from(init_data: &[u8]) -> Result<Self, Self::Error> {
        WidevinePsshData::decode(&init_data[32..])
            .or_else(|_| WidevinePsshData::decode(init_data))
            .or_else(|_| {
                let pssh_box =
                    PSSHBox::try_from(init_data).map_err(|e| Self::Error::new(e.to_string()))?;
                let data = pssh_box.data.unwrap_or_default();
                WidevinePsshData::decode(data.as_ref())
            })
    }
}

#[cfg(feature = "mpd")]
impl TryFrom<dash_mpd::MPD> for WidevineInitData {
    type Error = anyhow::Error;

    fn try_from(mpd: dash_mpd::MPD) -> Result<Self, Self::Error> {
        for period in mpd.periods.iter() {
            for adaption in period.adaptations.iter() {
                for protection in adaption.ContentProtection.iter() {
                    let scheme = protection.schemeIdUri.to_ascii_lowercase();
                    if scheme == WIDEVINE_SCHEME_ID_URI {
                        let text = protection.cenc_pssh[0].content.clone().unwrap();
                        return Ok(WidevineInitData(base64_decode(text)?));
                    }
                }

                for representation in adaption.representations.iter() {
                    for protection in representation.ContentProtection.iter() {
                        let scheme = protection.schemeIdUri.to_ascii_lowercase();
                        if scheme == WIDEVINE_SCHEME_ID_URI {
                            let text = protection.cenc_pssh[0].content.clone().unwrap();
                            return Ok(WidevineInitData(base64_decode(text)?));
                        }
                    }
                }
            }
        }

        bail!("pssh not found")
    }
}

impl Deref for WidevineInitData {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// This function retrieves certificate data from a given license server.
pub async fn get_cert_data(client: &reqwest::Client, license_url: &str) -> anyhow::Result<Bytes> {
    Ok(client
        .post(license_url)
        .body([0x08, 0x04].to_vec())
        .send()
        .await?
        .bytes()
        .await?)
}

pub fn extract_pssh_from_mp4(input: &[u8]) -> anyhow::Result<&[u8]> {
    let mut buf = Cursor::new(input);
    while buf.position() < buf.get_ref().len() as u64 {
        let box_size = buf.read_u32::<BigEndian>()?;
        let box_type = buf.read_u32::<BigEndian>()?;

        // moov
        if box_type == 0x6d6f6f76 {
            let moov_data = &buf.get_ref()
                [buf.position() as usize..(buf.position() + box_size as u64 - 8) as usize];
            let mut moov_reader = Cursor::new(moov_data);
            while moov_reader.position() < moov_reader.get_ref().len() as u64 {
                let box_size = moov_reader.read_u32::<BigEndian>()?;
                let box_type = moov_reader.read_u32::<BigEndian>()?;

                // pssh
                if box_type == 0x70737368 {
                    let pssh_data = &moov_reader.get_ref()[(moov_reader.position() - 8) as usize
                        ..(moov_reader.position() + box_size as u64) as usize];
                    return Ok(pssh_data);
                }
                moov_reader.set_position(moov_reader.position() + box_size as u64 - 8);
            }
        }
        buf.set_position(buf.position() + box_size as u64 - 8);
    }

    bail!("pssh not found")
}
