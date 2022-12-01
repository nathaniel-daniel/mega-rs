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
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct FileKey(pub [u8; KEY_SIZE]);

impl FileKey {
    /// Make a FileKey from encoded bytes
    pub(crate) fn from_encoded_bytes(input: &[u8; KEY_SIZE * 2]) -> Self {
        let (n1, n2) = input.split_at(KEY_SIZE);
        let n1 = u128::from_ne_bytes(n1.try_into().unwrap());
        let n2 = u128::from_ne_bytes(n2.try_into().unwrap());
        let key = n1 ^ n2;

        Self(key.to_ne_bytes())
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
