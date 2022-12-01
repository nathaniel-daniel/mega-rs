const KEY_SIZE: usize = 16;

/// An error that may occur while parsing a FolderKey.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// An error occured while decoding base64
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    /// The key is the wrong size
    #[error("invalid key length '{length}', expected length of '{KEY_SIZE}'")]
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
        let input = base64::decode_config(input, base64::URL_SAFE)?;

        let length = input.len();
        if length != KEY_SIZE {
            return Err(ParseError::InvalidLength { length });
        }

        // Length check is done earlier
        let key = u128::from_ne_bytes(input.try_into().unwrap());

        Ok(Self(key))
    }
}
