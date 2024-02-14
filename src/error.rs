use thiserror::Error;

#[derive(Error, Debug)]
pub enum IoriError {
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
}

pub type IoriResult<T> = Result<T, IoriError>;
