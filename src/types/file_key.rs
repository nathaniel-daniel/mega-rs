const KEY_SIZE: usize = 16;

/// An error that may occur while parsing a FileKey.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// An error occured while decoding base64
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    /// The key is the wrong size
    #[error("invalid key length '{length}'")]
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
    /// Make a FileKey from encoded bytes
    pub(crate) fn from_encoded_bytes(input: &[u8; KEY_SIZE * 2]) -> Self {
        let key = {
            let (n1, n2) = input.split_at(KEY_SIZE);

            // Lengths are verified via split above and the function array size limit
            let n1 = u128::from_ne_bytes(n1.try_into().unwrap());
            let n2 = u128::from_ne_bytes(n2.try_into().unwrap());

            n1 ^ n2
        };

        let iv = {
            let mut iv = [0; KEY_SIZE];
            iv[..8].copy_from_slice(&input[4 * 4..6 * 4]);
            u128::from_ne_bytes(iv)
        };

        let meta_mac = {
            let mut meta_mac = [0; std::mem::size_of::<u64>()];
            meta_mac.copy_from_slice(&input[6 * 4..8 * 4]);
            u64::from_ne_bytes(meta_mac)
        };

        Self { key, iv, meta_mac }
    }
}

impl std::str::FromStr for FileKey {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = base64::decode_config(input, base64::URL_SAFE)?;
        let length = input.len();
        if length != 2 * KEY_SIZE {
            return Err(ParseError::InvalidLength { length });
        }

        Ok(Self::from_encoded_bytes(
            input.as_slice().try_into().unwrap(),
        ))
    }
}
