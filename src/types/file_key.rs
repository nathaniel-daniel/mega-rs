use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

const KEY_SIZE: usize = 16;
const BASE64_LEN: usize = 43;
const BASE64_DECODE_BUFFER_LEN: usize = ((BASE64_LEN * 2) + 3) / 4 * 3;

/// An error that may occur while parsing a FileKey.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The base64 string is the wrong size
    #[error("invalid base64 length \"{length}\", expected length \"{BASE64_LEN}\"")]
    InvalidBase64Length { length: usize },

    /// An error occured while decoding base64
    #[error("base64 decode error")]
    Base64Decode(#[from] base64::DecodeSliceError),

    /// The key is the wrong size
    #[error("invalid key length \"{length}\", expected length \"{KEY_SIZE}\"")]
    InvalidLength { length: usize },
}

/// The encryption key for a file.
///
/// This includes:
/// * The 128 bit AES key
/// * The IV
/// * The meta mac
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct FileKey {
    /// The 128 bit AES key
    pub key: u128,

    /// The IV
    pub iv: u128,

    /// The meta mac
    pub meta_mac: u64,
}

impl FileKey {
    /// Make a FileKey from encoded bytes.
    pub(crate) fn from_encoded_bytes(input: &[u8; KEY_SIZE * 2]) -> Self {
        let key = {
            let (n1, n2) = input.split_at(KEY_SIZE);

            // Lengths are verified via split above and the function array size limit
            let n1 = u128::from_be_bytes(n1.try_into().unwrap());
            let n2 = u128::from_be_bytes(n2.try_into().unwrap());

            n1 ^ n2
        };

        let (iv, meta_mac) = input[KEY_SIZE..].split_at(std::mem::size_of::<u64>());

        // Length is verified by split above.
        let iv = u128::from(u64::from_be_bytes(iv.try_into().unwrap())) << 64;

        // Length is verified by split and length of input.
        let meta_mac = u64::from_be_bytes(meta_mac.try_into().unwrap());

        Self { key, iv, meta_mac }
    }
}

impl std::str::FromStr for FileKey {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let length = input.len();
        if length != BASE64_LEN {
            return Err(ParseError::InvalidBase64Length { length });
        }

        let mut base64_decode_buffer = [0; BASE64_DECODE_BUFFER_LEN];
        let decoded_len = URL_SAFE_NO_PAD.decode_slice(input, &mut base64_decode_buffer)?;
        let input = &base64_decode_buffer[..decoded_len];
        let length = input.len();
        if length != KEY_SIZE * 2 {
            return Err(ParseError::InvalidLength { length });
        }

        // Length is checked above
        Ok(Self::from_encoded_bytes(input.try_into().unwrap()))
    }
}
