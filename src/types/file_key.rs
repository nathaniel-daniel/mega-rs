const KEY_SIZE: usize = 16;

/// An error that may occur while parsing a FileKey.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// An error occured while decoding base64
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    /// The key is the wrong size
    #[error("invalid key length")]
    InvalidLength,
}

/// The encryption key for a file.
#[derive(Debug)]
pub struct FileKey(pub [u8; KEY_SIZE]);

impl std::str::FromStr for FileKey {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = base64::decode_config(input, base64::URL_SAFE)?;
        if input.len() != 2 * KEY_SIZE {
            return Err(ParseError::InvalidLength);
        }
        let mut key = [0; KEY_SIZE];
        for (key, (n1, n2)) in key
            .iter_mut()
            .zip(input[..KEY_SIZE].iter().zip(input[KEY_SIZE..].iter()))
        {
            *key = n1 ^ n2;
        }

        Ok(Self(key))
    }
}
