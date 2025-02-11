use std::sync::LazyLock;

use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::{DecodeError, Engine};

static ENGINE: LazyLock<GeneralPurpose> = LazyLock::new(|| {
    GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_encode_padding(true)
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    )
});

pub fn base64_decode<T: AsRef<[u8]>>(input: T) -> Result<Vec<u8>, DecodeError> {
    ENGINE.decode(input)
}
