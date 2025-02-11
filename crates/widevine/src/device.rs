use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockEncryptMut, KeyIvInit};
use reqwest::Client;
use std::path::Path;

use crate::base64::base64_decode;
use crate::constants::COMMON_PRIVACY_CERT;
use crate::protocol::{
    ClientIdentification, DrmCertificate, EncryptedClientIdentification, SignedDrmCertificate,
    SignedMessage,
};
use crate::traits::IntoLicenseHeaders;
use prost::Message;
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey},
    Oaep, Pss, RsaPrivateKey, RsaPublicKey,
};

use super::{pssh::WidevineInitData, session::Session};

pub struct Device {
    pub client_id: ClientIdentification,
    pub(crate) private_key: RsaPrivateKey,
}

impl Device {
    pub fn new<P>(base: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path>,
    {
        let client_id_path = base.as_ref().join("client_id.bin");
        let client_id = std::fs::read(client_id_path)?;
        let client_id = ClientIdentification::decode(client_id.as_ref())?;

        let private_key_path = base.as_ref().join("private_key.pem");
        let private_key = std::fs::read_to_string(private_key_path)?;
        let private_key = RsaPrivateKey::from_pkcs1_pem(&private_key)?;

        Ok(Self {
            client_id,
            private_key,
        })
    }

    pub fn new_static(client_id: &[u8], private_key: &str) -> anyhow::Result<Self> {
        let client_id = ClientIdentification::decode(client_id)?;
        let private_key = RsaPrivateKey::from_pkcs1_pem(private_key)?;

        Ok(Self {
            client_id,
            private_key,
        })
    }

    pub fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>, rsa::Error> {
        let padding = Oaep::new::<sha1::Sha1>();
        self.private_key.decrypt(padding, input)
    }

    pub fn sign(&self, input: &[u8]) -> Result<Vec<u8>, rsa::Error> {
        let mut rng = rand::thread_rng();
        let padding = Pss::new_blinded::<sha1::Sha1>();
        self.private_key.sign_with_rng(&mut rng, padding, input)
    }

    pub fn open(&self, init_data: WidevineInitData) -> anyhow::Result<Session> {
        Ok(Session::new(self, init_data.get_pssh()?))
    }

    /// Convenient method to fetch keys
    pub async fn fetch_key<S, H>(
        &self,
        init_data: WidevineInitData,
        license_url: S,
        license_headers: H,
        service_certificate: Option<ServerCertificate>,
    ) -> anyhow::Result<String>
    where
        S: AsRef<str>,
        H: IntoLicenseHeaders,
    {
        let mut session = self.open(init_data)?;
        session.set_service_certificate(service_certificate)?;
        let license_request = session.get_license_request();

        let license_response = Client::new()
            .post(license_url.as_ref())
            .headers(license_headers.into_license_headers())
            .body(license_request.encode_to_vec())
            .send()
            .await?
            .bytes()
            .await?;
        let keys = session.get_license_keys(license_request, &license_response)?;
        let key = keys
            .iter()
            .find(|k| k.r#type == "CONTENT")
            .ok_or_else(|| anyhow::anyhow!("no CONTENT key found"))?;
        Ok(format!("{}:{}", key.id, key.key))
    }
}

#[derive(Debug)]
pub struct ServerCertificate {
    certificate: DrmCertificate,
    // signed_certificate: SignedDrmCertificate,
}

impl ServerCertificate {
    pub fn new(message: SignedMessage) -> anyhow::Result<Self> {
        let message = message.msg();
        let signed_certificate = SignedDrmCertificate::decode(message)?;

        // TODO: verify signature
        let certificate = signed_certificate.drm_certificate();
        let certificate = DrmCertificate::decode(certificate)?;

        Ok(Self {
            certificate,
            // signed_certificate,
        })
    }

    pub fn from_base64(input: &str) -> anyhow::Result<Self> {
        let buf = base64_decode(input)?;
        Self::from_raw(buf.as_slice())
    }

    pub fn from_raw(input: &[u8]) -> anyhow::Result<Self> {
        let message = SignedMessage::decode(input)?;
        Self::new(message)
    }

    pub fn get_client_id(&self, client_id: &ClientIdentification) -> EncryptedClientIdentification {
        let privacy_key: [u8; 16] = rand::random();
        let privacy_iv: [u8; 16] = rand::random();

        let encrypt = cbc::Encryptor::<aes::Aes128>::new(&privacy_key.into(), &privacy_iv.into());
        let client_id = encrypt.encrypt_padded_vec_mut::<Pkcs7>(&client_id.encode_to_vec());

        let public_key = RsaPublicKey::from_pkcs1_der(self.certificate.public_key()).unwrap();
        let padding = Oaep::new::<sha1::Sha1>();
        let mut rng = rand::thread_rng();
        let privacy_key = public_key.encrypt(&mut rng, padding, &privacy_key).unwrap();

        EncryptedClientIdentification {
            provider_id: self.certificate.provider_id.clone(),
            service_certificate_serial_number: self.certificate.serial_number.clone(),
            encrypted_client_id: Some(client_id),
            encrypted_privacy_key: Some(privacy_key),
            encrypted_client_id_iv: Some(privacy_iv.to_vec()),
        }
    }
}

impl Default for ServerCertificate {
    fn default() -> Self {
        ServerCertificate::new(SignedMessage::decode(COMMON_PRIVACY_CERT.as_ref()).unwrap())
            .unwrap()
    }
}

#[cfg(test)]
mod test {
    use crate::constants::ROOT_SIGNED_CERT;
    use crate::device::ServerCertificate;
    use crate::protocol::SignedDrmCertificate;
    use prost::Message;

    #[test]
    fn test_default_server_certificate() {
        let _ = ServerCertificate::default();
    }

    #[test]
    fn test_common_privacy_cert() -> anyhow::Result<()> {
        let _ = SignedDrmCertificate::decode(ROOT_SIGNED_CERT.as_ref())?;
        Ok(())
    }
}
