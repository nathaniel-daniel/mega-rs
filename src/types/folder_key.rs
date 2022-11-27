const KEY_SIZE: usize = 16;

/// An error that may occur while parsing a FolderKey.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// An error occured while decoding base64
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    /// The key is the wrong size
    #[error("invalid key length '{length}'")]
    InvalidLength { length: usize },
}

/// The encryption key for a folder.
#[derive(Debug)]
pub struct FolderKey(pub [u8; KEY_SIZE]);

impl std::str::FromStr for FolderKey {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = base64::decode_config(input, base64::URL_SAFE)?;
        let length = input.len();
        if length != KEY_SIZE {
            return Err(ParseError::InvalidLength { length });
        }
        let mut key = [0; KEY_SIZE];
        key.copy_from_slice(input.as_slice());

        Ok(Self(key))
    }
}
