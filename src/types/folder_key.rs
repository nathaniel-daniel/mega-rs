use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

const KEY_SIZE: usize = 16;
const BASE64_LEN: usize = 22;
const BASE64_DECODE_BUFFER_LEN: usize = ((BASE64_LEN * 2) + 3) / 4 * 3;

/// An error that may occur while parsing a FolderKey.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The base64 string is the wrong size
    #[error("invalid base64 length \"{length}\", expected length of \"{BASE64_LEN}\"")]
    InvalidBase64Length { length: usize },

    /// An error occured while decoding base64
    #[error("base64 decode error")]
    Base64Decode(#[from] base64::DecodeSliceError),

    /// The key is the wrong size
    #[error("invalid key length \"{length}\", expected length of \"{KEY_SIZE}\"")]
    InvalidLength { length: usize },
}

/// The encryption key for a folder.
///
/// This is a 128 bit AES key.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct FolderKey(pub u128);

impl std::str::FromStr for FolderKey {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let length = input.len();
        if length < BASE64_LEN {
            return Err(ParseError::InvalidBase64Length { length });
        }

        let mut base64_decode_buffer = [0; BASE64_DECODE_BUFFER_LEN];
        let decoded_len = URL_SAFE_NO_PAD.decode_slice(input, &mut base64_decode_buffer)?;
        let input = &base64_decode_buffer[..decoded_len];

        let length = input.len();
        if length != KEY_SIZE {
            return Err(ParseError::InvalidLength { length });
        }

        // Length check is done earlier
        let key = u128::from_ne_bytes(input.try_into().unwrap());

        Ok(Self(key))
    }
}
