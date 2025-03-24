use crate::ErrorCode;
use crate::FileKey;
use crate::FolderKey;
use crate::FolderKeyParseError;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use cbc::cipher::BlockDecryptMut;
use cbc::cipher::KeyInit;
use cbc::cipher::KeyIvInit;
use std::collections::HashMap;
use url::Url;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes128EcbDec = ecb::Decryptor<aes::Aes128>;

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
    /// Convert this into a Result.
    pub fn into_result(self) -> Result<T, ErrorCode> {
        match self {
            Self::Ok(t) => Ok(t),
            Self::Error(error) => Err(error),
        }
    }
}

/// API Response data
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum ResponseData {
    /// Response for a GetAttributes command
    GetAttributes(GetAttributes),

    /// Response for FetchNodes command
    FetchNodes(FetchNodes),
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

    /// Failed to parse a folder key
    #[error("failed to parse folder key")]
    ParseFolderKey(#[from] FolderKeyParseError),

    /// A key was missing a header
    #[error("key missing header")]
    KeyMissingHeader,

    /// The key was the wrong size
    #[error("invalid key length '{length}'")]
    InvalidKeyLength { length: usize },
}

/// File attributes
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct FileAttributes {
    /// The name of the file
    #[serde(rename = "n")]
    pub name: String,

    /// ?
    pub c: Option<String>,

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

    /// Unknown attributes
    #[serde(flatten)]
    pub unknown: HashMap<String, serde_json::Value>,
}

impl GetAttributes {
    /// Decode the encoded attributes
    pub fn decode_attributes(&self, key: u128) -> Result<FileAttributes, DecodeAttributesError> {
        decode_attributes(&self.encoded_attributes, key)
    }
}

/// FetchNodes command response
#[derive(Debug, serde::Serialize, serde:: Deserialize)]
pub struct FetchNodes {
    #[serde(rename = "f")]
    pub files: Vec<FetchNodesNode>,

    pub noc: u8,

    pub sn: String,
    pub st: String,

    /// Unknown attributes
    #[serde(flatten)]
    pub unknown: HashMap<String, serde_json::Value>,
}

/// The kind of node
#[derive(
    Debug,
    Eq,
    PartialEq,
    Hash,
    Copy,
    Clone,
    serde_repr::Deserialize_repr,
    serde_repr::Serialize_repr,
)]
#[repr(u8)]
pub enum FetchNodesNodeKind {
    /// A file
    File = 0,

    /// A directory
    Directory = 1,

    /// The special root directory
    Root = 2,

    /// The special inbox directory
    Inbox = 3,

    /// The special trash bin directory
    TrashBin = 4,
}

impl FetchNodesNodeKind {
    /// Returns true if this is a file.
    pub fn is_file(self) -> bool {
        matches!(self, Self::File)
    }

    /// Returns true if this is a dir.
    pub fn is_dir(self) -> bool {
        matches!(self, Self::Directory)
    }

    /// Returns true if this is a root.
    pub fn is_root(self) -> bool {
        matches!(self, Self::Root)
    }

    /// Returns true if this is an inbox.
    pub fn is_inbox(self) -> bool {
        matches!(self, Self::Inbox)
    }

    /// Returns true if this is a trash bin.
    pub fn is_trash_bin(self) -> bool {
        matches!(self, Self::TrashBin)
    }
}

/// A FetchNodes Node
#[derive(Debug, serde::Serialize, serde:: Deserialize)]
pub struct FetchNodesNode {
    /// The attributes of the node
    #[serde(rename = "a")]
    pub encoded_attributes: String,

    /// The id of the node
    #[serde(rename = "h")]
    pub id: String,

    /// The key of the node
    #[serde(rename = "k")]
    pub key: String,

    /// The id of the parent node
    #[serde(rename = "p")]
    pub parent_id: String,

    /// The kind of the node
    #[serde(rename = "t")]
    pub kind: FetchNodesNodeKind,

    /// The time of last modification
    #[serde(rename = "ts")]
    pub timestamp: u64,

    /// The owner of the node
    #[serde(rename = "u")]
    pub user: String,

    pub fa: Option<String>,

    /// The size of the node
    #[serde(rename = "s")]
    pub size: Option<u64>,

    /// Unknown attributes
    #[serde(flatten)]
    pub unknown: HashMap<String, serde_json::Value>,
}

impl FetchNodesNode {
    /// Decode the encoded attributes
    pub fn decode_attributes(
        &self,
        folder_key: &FolderKey,
    ) -> Result<FileAttributes, DecodeAttributesError> {
        let (_, key) = self
            .key
            .split_once(':')
            .ok_or(DecodeAttributesError::KeyMissingHeader)?;

        let mut key = URL_SAFE_NO_PAD.decode(key)?;
        let cipher = Aes128EcbDec::new(&folder_key.0.to_ne_bytes().into());
        let key = cipher
            .decrypt_padded_mut::<block_padding::NoPadding>(&mut key)
            .map_err(DecodeAttributesError::Decrypt)?;
        let key_len = key.len();
        let key: u128 = if self.kind == FetchNodesNodeKind::Directory {
            if key_len != 16 {
                return Err(DecodeAttributesError::InvalidKeyLength { length: key_len });
            }

            // Length check is done above
            u128::from_ne_bytes(key.try_into().unwrap())
        } else {
            if key_len != 32 {
                return Err(DecodeAttributesError::InvalidKeyLength { length: key_len });
            }

            // Length check is done above
            FileKey::from_encoded_bytes(key.try_into().unwrap()).key
        };

        decode_attributes(&self.encoded_attributes, key)
    }

    /// Check if this is a file.
    pub fn is_file(&self) -> bool {
        self.kind.is_file()
    }

    /// Check if this is a dir.
    pub fn is_dir(&self) -> bool {
        self.kind.is_dir()
    }
}

/// Decode the encoded attributes
fn decode_attributes(
    encoded_attributes: &str,
    key: u128,
) -> Result<FileAttributes, DecodeAttributesError> {
    let mut encoded_attributes = URL_SAFE_NO_PAD.decode(encoded_attributes)?;

    let cipher = Aes128CbcDec::new(&key.to_ne_bytes().into(), &[0; 16].into());
    let decrypted = cipher
        .decrypt_padded_mut::<block_padding::ZeroPadding>(&mut encoded_attributes)
        .map_err(DecodeAttributesError::Decrypt)?;

    let decrypted = std::str::from_utf8(decrypted)?;
    let decrypted = decrypted
        .strip_prefix("MEGA")
        .ok_or(DecodeAttributesError::MissingMegaPrefix)?;

    Ok(serde_json::from_str(decrypted)?)
}
