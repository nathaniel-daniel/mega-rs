use super::file_key::BASE64_LEN as FILE_KEY_BASE64_LEN;
use super::folder_key::BASE64_LEN as FOLDER_KEY_BASE64_LEN;
use crate::FileKey;
use crate::FolderKey;

/// An error that may occur while parsing a FileOrFolderKey.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// A file error
    #[error("file key parse error")]
    File(#[from] super::file_key::ParseError),

    /// A folder error
    #[error("folder key parse error")]
    Folder(#[from] super::folder_key::ParseError),

    /// Invalid key len
    #[error("invalid key length {0}")]
    InvalidKeyLength(usize),
}

/// Either a file or folder key
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum FileOrFolderKey {
    File(FileKey),
    Folder(FolderKey),
}

impl FileOrFolderKey {
    /// Get the key.
    pub fn key(&self) -> u128 {
        match self {
            Self::File(file_key) => file_key.key,
            Self::Folder(folder_key) => folder_key.0,
        }
    }

    /// Get a ref to the file key, if it is one.
    pub fn as_file_key(&self) -> Option<&FileKey> {
        match self {
            Self::File(key) => Some(key),
            _ => None,
        }
    }

    /// Get a ref to the folder key, if it is one.
    pub fn as_folder_key(&self) -> Option<&FolderKey> {
        match self {
            Self::Folder(key) => Some(key),
            _ => None,
        }
    }

    /// Take the file key, if it is one.
    pub fn take_file_key(self) -> Option<FileKey> {
        match self {
            Self::File(key) => Some(key),
            _ => None,
        }
    }

    /// Take the folder key, if it is one.
    pub fn take_folder_key(self) -> Option<FolderKey> {
        match self {
            Self::Folder(key) => Some(key),
            _ => None,
        }
    }

    /// Check if this is a file key.
    pub fn is_file_key(&self) -> bool {
        matches!(self, Self::File(_))
    }

    /// Check if this is a folder key.
    pub fn is_folder_key(&self) -> bool {
        matches!(self, Self::Folder(_))
    }
}

impl std::str::FromStr for FileOrFolderKey {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.len() {
            FILE_KEY_BASE64_LEN => Ok(Self::File(input.parse()?)),
            FOLDER_KEY_BASE64_LEN => Ok(Self::Folder(input.parse()?)),
            len => Err(ParseError::InvalidKeyLength(len)),
        }
    }
}

impl std::fmt::Display for FileOrFolderKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::File(file_key) => file_key.fmt(f),
            Self::Folder(folder_key) => folder_key.fmt(f),
        }
    }
}

impl From<FileKey> for FileOrFolderKey {
    fn from(key: FileKey) -> Self {
        Self::File(key)
    }
}

impl From<FolderKey> for FileOrFolderKey {
    fn from(key: FolderKey) -> Self {
        Self::Folder(key)
    }
}

impl From<FileOrFolderKey> for String {
    fn from(key: FileOrFolderKey) -> Self {
        key.to_string()
    }
}

impl TryFrom<String> for FileOrFolderKey {
    type Error = <Self as std::str::FromStr>::Err;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
