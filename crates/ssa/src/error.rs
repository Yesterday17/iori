#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("MPEG-TS error: {0}")]
    MpegTsError(#[from] mpeg2ts::Error),

    #[error("Invalid NAL unit start code")]
    InvalidStartCode,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("ID3 error: {0}")]
    Id3Error(#[from] id3::Error),
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
