use std::time::{SystemTime, UNIX_EPOCH};

use crate::device::ServerCertificate;
use crate::key::WidevineKey;
use crate::protocol::{
    license_request::content_identification::{
        ContentIdVariant, WidevinePsshData as WidevineMessagePsshData,
    },
    license_request::{self, ContentIdentification},
    signed_message::MessageType,
    License, LicenseRequest, LicenseType, ProtocolVersion, SignedMessage, WidevinePsshData,
};
use aes::cipher::{BlockDecryptMut, KeyIvInit};
use anyhow::bail;
use cbc::cipher::block_padding::Pkcs7;
use cmac::{Cmac, Mac};
use prost::Message;
use sha1::{Digest, Sha1};

// https://github.com/nilaoda/WVCore/blob/main/Widevine/Session.cs
use super::device::Device;

pub struct Session<'device> {
    device: &'device Device,
    pssh: WidevinePsshData,

    // privacy mode
    server_certificate: Option<ServerCertificate>,
}

impl<'d> Session<'d> {
    pub fn new(device: &'d Device, pssh: WidevinePsshData) -> Self {
        Self {
            device,
            pssh,
            server_certificate: None,
        }
    }

    pub fn set_service_certificate(
        &mut self,
        cert: Option<ServerCertificate>,
    ) -> anyhow::Result<()> {
        self.server_certificate = cert;
        Ok(())
    }

    pub fn clear_service_certificate(&mut self) {
        self.server_certificate = None;
    }

    pub fn get_license_request(&self) -> SignedMessage {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let (client_id, encrypted_client_id) = if let Some(r) = &self.server_certificate {
            (None, Some(r.get_client_id(&self.device.client_id)))
        } else {
            (Some(self.device.client_id.clone()), None)
        };

        let license_request = LicenseRequest {
            client_id,
            encrypted_client_id,
            content_id: Some(ContentIdentification {
                content_id_variant: Some(ContentIdVariant::WidevinePsshData(
                    WidevineMessagePsshData {
                        pssh_data: vec![self.pssh.encode_to_vec()],
                        license_type: Some(LicenseType::Streaming.into()),
                        request_id: Some({
                            let mut id = [0u8; 16];
                            id[0..4].copy_from_slice(&rand::random::<[u8; 4]>());
                            let index = 5u64; // FIXME: use session number instead of fixed 5
                            id[8..16].copy_from_slice(&index.to_le_bytes());

                            hex::encode_upper(id).into_bytes()
                        }),
                    },
                )),
            }),
            r#type: Some(license_request::RequestType::New.into()),
            request_time: Some(now as i64),
            protocol_version: Some(ProtocolVersion::Version21.into()),
            key_control_nonce: Some(rand::random()),
            key_control_nonce_deprecated: None,
        };

        let request_buffer = license_request.encode_to_vec();
        let mut hasher = Sha1::new();
        hasher.update(&request_buffer);
        let result = hasher.finalize();
        let signature = self.device.sign(result.as_ref()).unwrap();

        SignedMessage {
            r#type: Some(MessageType::LicenseRequest.into()),
            msg: Some(request_buffer),
            signature: Some(signature),
            ..Default::default()
        }
    }

    pub fn get_license_keys(
        &self,
        request: SignedMessage,
        response: &[u8],
    ) -> anyhow::Result<Vec<WidevineKey>> {
        let license_message = SignedMessage::decode(response)
            .map_err(|_| anyhow::anyhow!("{:?}", response))
            .unwrap();
        match license_message.r#type {
            Some(2 /* MessageType::License */) => {} // accepted
            Some(r#type) => bail!(
                "Expecting a LICENSE message, not a '{}' message.",
                MessageType::try_from(r#type).unwrap().as_str_name()
            ),
            None => bail!("Expecting a LICENSE message, not an UNKNOWN message."),
        }
        let license = License::decode(license_message.msg())?;

        let context = request.msg();
        let (enc_context, mac_context) = Self::derive_context(context);
        let (enc_key, _, _) = Self::derive_keys(
            &enc_context,
            &mac_context,
            self.device.decrypt(license_message.session_key())?.as_ref(),
        );

        assert_eq!(enc_key.len(), 16);
        let enc_key: [u8; 16] = enc_key.try_into().unwrap();
        // TODO: validate signature
        // let computed_signature =

        let mut result_keys = Vec::with_capacity(license.key.len());
        for key in license.key {
            let kid = key.id();
            let decrypt = cbc::Decryptor::<aes::Aes128>::new(&enc_key.into(), key.iv().into());
            let result = decrypt.decrypt_padded_vec_mut::<Pkcs7>(key.key())?;

            result_keys.push(WidevineKey {
                r#type: key.r#type().as_str_name(),
                id: hex::encode(kid),
                key: hex::encode(result),
            });
        }

        Ok(result_keys)
    }

    /// Returns 2 Context Data used for computing the AES Encryption and HMAC Keys.
    fn derive_context(message: &[u8]) -> (Vec<u8>, Vec<u8>) {
        fn get_enc_context(input: &[u8]) -> Vec<u8> {
            let mut result: Vec<u8> = Vec::with_capacity(10 + 1 + input.len() + 4);
            result.extend_from_slice(b"ENCRYPTION\x00");
            result.extend_from_slice(input);
            let key_size: u32 = 16 * 8; // 128-bit
            let key_size = key_size.to_be_bytes();
            result.extend_from_slice(&key_size);
            result
        }

        fn get_mac_context(input: &[u8]) -> Vec<u8> {
            let mut result = Vec::with_capacity(14 + 1 + input.len() + 4);
            result.extend_from_slice(b"AUTHENTICATION\x00");
            result.extend_from_slice(input);
            let key_size: u32 = 32 * 8 * 2; // 512-bit
            let key_size = key_size.to_be_bytes();
            result.extend_from_slice(&key_size);

            result
        }

        (get_enc_context(message), get_mac_context(message))
    }

    fn derive_keys(
        enc_context: &[u8],
        mac_context: &[u8],
        key: &[u8],
    ) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        fn derive(session_key: &[u8], context: &[u8], counter: u8) -> Vec<u8> {
            let mut cmac = Cmac::<aes::Aes128>::new_from_slice(session_key.as_ref()).unwrap();
            cmac.update(&[counter]);
            cmac.update(context);
            cmac.finalize().into_bytes().to_vec()
        }

        let enc_key = derive(key, enc_context, 1);

        let mut mac_key_server = derive(key, mac_context, 1);
        let mut mac_key_server2 = derive(key, mac_context, 2);
        mac_key_server.append(&mut mac_key_server2);

        let mut mac_key_client = derive(key, mac_context, 3);
        let mut mac_key_client2 = derive(key, mac_context, 4);
        mac_key_client.append(&mut mac_key_client2);

        (enc_key, mac_key_server, mac_key_client)
    }
}
