use aes::cipher::block_padding::UnpadError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IoriError {
    #[error("HTTP error: {0}")]
    HttpError(reqwest::StatusCode),

    #[error("M3u8 fetch error")]
    M3u8FetchError,

    #[error("Invalid clear key: {0}")]
    InvalidClearKey(String),

    #[error("Invalid AES-128 key: {0:?}")]
    InvalidAes128Key(Vec<u8>),

    #[error("mp4decrypt error: {0}")]
    Mp4DecryptError(String),

    #[error("iori-ssa error: {0:?}")]
    IoriSsaError(#[from] iori_ssa::Error),

    #[error("Pkcs7 unpad error")]
    UnpadError(#[from] UnpadError),

    #[error("Invalid m3u8 file: {0}")]
    M3u8ParseError(String),

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error(transparent)]
    HexDecodeError(#[from] hex::FromHexError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    // MPEG-DASH errors
    #[error(transparent)]
    MpdParseError(#[from] dash_mpd::DashMpdError),

    #[error("Invalid timing schema: {0:?}")]
    InvalidTimingSchema(String),

    #[error(transparent)]
    MissingExecutable(#[from] which::Error),

    #[error("Can not set cache directory to an existing path: {0}")]
    CacheDirExists(std::path::PathBuf),
}

pub type IoriResult<T> = Result<T, IoriError>;
