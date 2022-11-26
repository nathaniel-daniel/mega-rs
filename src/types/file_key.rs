/// An error that may occur while parsing a FileKey.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// An error occured while decoding base64
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    /// The key is the wrong size
    #[error("invalid key length")]
    InvalidLength(#[from] std::array::TryFromSliceError),
}

/// The encryption key for a file.
///
/// This contains a key/iv pair.
#[derive(Debug)]
pub struct FileKey(pub [u8; 32]);

impl FileKey {
    /// Get the file key.
    pub fn get_key(&self) -> &[u8; 16] {
        self.0[..16].try_into().unwrap()
    }

    /// Get the file iv.
    pub fn get_iv(&self) -> &[u8; 16] {
        self.0[16..].try_into().unwrap()
    }
}

impl std::str::FromStr for FileKey {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = base64::decode_config(input, base64::URL_SAFE)?;
        Ok(Self(input.as_slice().try_into()?))
    }
}
