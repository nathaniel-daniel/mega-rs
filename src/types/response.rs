use cbc::cipher::BlockDecryptMut;
use cbc::cipher::KeyIvInit;
use std::collections::HashMap;
use url::Url;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// An api response
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum Response<T> {
    /// Error
    ///
    /// There was en error with the specified code.
    Error(ErrorCode),

    /// Success
    ///
    /// There is valid data
    Ok(T),
}

impl<T> Response<T> {
    /// Unwrap the response, panicing on failure.
    ///
    /// Intended for quick testing and scripting.
    pub fn unwrap(self) -> T {
        match self {
            Self::Error(error) => panic!("Called 'unwrap' on Error({error:#?})"),
            Self::Ok(t) => t,
        }
    }
}

/// An API Error
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ErrorCode(i32);

/// API Response data
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum ResponseData {
    /// Response for a GetAttributes command
    GetAttributes(GetAttributes),
}

/// An error that may occur while decoding attributes
#[derive(Debug, thiserror::Error)]
pub enum DecodeAttributesError {
    /// Failed to decode base64
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    /// Decryption failed
    #[error("failed to decrypt")]
    Decrypt(block_padding::UnpadError),

    /// Invalid utf8
    #[error(transparent)]
    InvalidUtf8(#[from] std::str::Utf8Error),

    /// Missing the MEGA prefix
    #[error("missing MEGA prefix")]
    MissingMegaPrefix,

    /// Json parse error
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// File attributes
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct FileAttributes {
    /// The name of the file
    #[serde(rename = "n")]
    pub name: String,

    /// Unknown attributes
    #[serde(flatten)]
    pub unknown: HashMap<String, serde_json::Value>,
}

/// GetAttributes command response
#[derive(Debug, serde::Serialize, serde:: Deserialize)]
pub struct GetAttributes {
    /// The file size
    #[serde(rename = "s")]
    pub size: u64,

    /// Encoded attributes
    #[serde(rename = "at")]
    pub encoded_attributes: String,

    pub msd: u8,

    /// The download url
    #[serde(rename = "g")]
    pub download_url: Option<Url>,
}

impl GetAttributes {
    /// Decode the encoded attributes
    pub fn decode_attributes(
        &self,
        key: &[u8; 16],
    ) -> Result<FileAttributes, DecodeAttributesError> {
        let mut encoded_attributes =
            base64::decode_config(&self.encoded_attributes, base64::URL_SAFE)?;
        let cipher = Aes128CbcDec::new(key.into(), &[0; 16].into());
        let decrypted = cipher
            .decrypt_padded_mut::<block_padding::NoPadding>(&mut encoded_attributes)
            .map_err(DecodeAttributesError::Decrypt)?;
        let decrypted = std::str::from_utf8(decrypted)?;
        let decrypted = decrypted
            .strip_prefix("MEGA")
            .ok_or(DecodeAttributesError::MissingMegaPrefix)?
            .trim_end_matches('\0');
        Ok(serde_json::from_str(decrypted)?)
    }
}
