mod client;
#[cfg(feature = "easy")]
mod easy;
mod file_validator;
mod parsed_mega_url;
mod types;

pub use self::client::Client;
#[cfg(feature = "easy")]
pub use self::easy::Client as EasyClient;
#[cfg(feature = "easy")]
pub use self::easy::FileDownloadReader as EasyFileDownloadReader;
#[cfg(feature = "easy")]
pub use self::easy::GetAttributesBuilder as EasyGetAttributesBuilder;
pub use self::file_validator::FileValidationError;
pub use self::file_validator::FileValidator;
pub use self::parsed_mega_url::ParseMegaUrlError;
pub use self::parsed_mega_url::ParsedMegaFileUrl;
pub use self::parsed_mega_url::ParsedMegaFolderUrl;
pub use self::parsed_mega_url::ParsedMegaUrl;
pub use self::types::Command;
pub use self::types::DecodeAttributesError;
pub use self::types::ErrorCode;
pub use self::types::FetchNodesNodeKind;
pub use self::types::FetchNodesResponse;
pub use self::types::FileKey;
pub use self::types::FileKeyParseError;
pub use self::types::FolderKey;
pub use self::types::FolderKeyParseError;
pub use self::types::GetAttributesResponse;
pub use self::types::Response;
pub use self::types::ResponseData;
pub use url::Url;

/// The library error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A reqwest Error
    #[error("http error")]
    Reqwest(#[from] reqwest::Error),

    /// A Url Error
    #[error("url error")]
    Url(#[from] url::ParseError),

    /// The returned number of responses did not match what was expected
    #[error("expected \"{expected}\" responses, but got \"{actual}\"")]
    ResponseLengthMismatch { expected: usize, actual: usize },

    /// There was an api error
    #[error("api error")]
    ApiError(#[from] ErrorCode),

    /// Failed to decode attributes
    #[error("failed to decode attributes")]
    DecodeAttributes(#[from] DecodeAttributesError),

    #[cfg(feature = "easy")]
    #[error("channel closed without response")]
    NoResponse,

    #[cfg(feature = "easy")]
    #[error("error occured as part of a batched send")]
    BatchSend(self::easy::ArcError<Self>),

    #[cfg(feature = "easy")]
    #[error("unexpected response data type")]
    UnexpectedResponseDataType,
}

/// Either a file or folder key
#[derive(Debug, Clone)]
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

#[cfg(test)]
mod test {
    use super::*;
    use cbc::cipher::KeyIvInit;
    use cbc::cipher::StreamCipher;

    type Aes128Ctr128BE = ctr::Ctr128BE<aes::Aes128>;

    pub const TEST_FILE: &str =
        "https://mega.nz/file/7glwEQBT#Fy9cwPpCmuaVdEkW19qwBLaiMeyufB1kseqisOAxfi8";
    pub const TEST_FILE_KEY: &str = "Fy9cwPpCmuaVdEkW19qwBLaiMeyufB1kseqisOAxfi8";
    pub const TEST_FILE_ID: &str = "7glwEQBT";

    pub const TEST_FOLDER: &str = "https://mega.nz/folder/MWsm3aBL#xsXXTpoYEFDRQdeHPDrv7A";
    pub const TEST_FOLDER_KEY: &str = "xsXXTpoYEFDRQdeHPDrv7A";
    pub const TEST_FOLDER_ID: &str = "MWsm3aBL";

    pub const TEST_FOLDER_NESTED: &str =
        "https://mega.nz/folder/MWsm3aBL#xsXXTpoYEFDRQdeHPDrv7A/folder/IGlBlD6K";

    pub const TEST_FILE_KEY_KEY_DECODED: u128 = u128::from_be_bytes([
        161, 141, 109, 44, 84, 62, 135, 130, 36, 158, 235, 166, 55, 235, 206, 43,
    ]);
    pub const TEST_FILE_KEY_IV_DECODED: u128 =
        u128::from_be_bytes([182, 162, 49, 236, 174, 124, 29, 100, 0, 0, 0, 0, 0, 0, 0, 0]);
    pub const TEST_FILE_META_MAC_DECODED: u64 =
        u64::from_be_bytes([177, 234, 162, 176, 224, 49, 126, 47]);
    pub const TEST_FOLDER_KEY_DECODED: u128 = u128::from_be_bytes([
        198, 197, 215, 78, 154, 24, 16, 80, 209, 65, 215, 135, 60, 58, 239, 236,
    ]);

    pub const TEST_FILE_BYTES: &[u8] = include_bytes!("../test_data/Doxygen_docs.zip");

    #[test]
    fn parse_file_key() {
        let file_key: FileKey = TEST_FILE_KEY.parse().expect("failed to parse file key");
        assert!(file_key.key == TEST_FILE_KEY_KEY_DECODED);
        assert!(file_key.iv == TEST_FILE_KEY_IV_DECODED);
        assert!(file_key.meta_mac == TEST_FILE_META_MAC_DECODED);
    }

    #[test]
    fn parse_folder_key() {
        let folder_key: FolderKey = TEST_FOLDER_KEY.parse().expect("failed to parse folder key");
        assert!(folder_key.0 == TEST_FOLDER_KEY_DECODED);
    }

    #[tokio::test]
    async fn download_file() {
        let file_key = FileKey {
            key: TEST_FILE_KEY_KEY_DECODED,
            iv: TEST_FILE_KEY_IV_DECODED,
            meta_mac: TEST_FILE_META_MAC_DECODED,
        };

        let client = Client::new();
        let commands = vec![Command::GetAttributes {
            public_node_id: Some(TEST_FILE_ID.into()),
            node_id: None,
            include_download_url: Some(1),
        }];
        let mut response = client
            .execute_commands(&commands, None)
            .await
            .expect("failed to execute commands");
        assert!(response.len() == 1);
        let response = response.swap_remove(0);
        let response = response.into_result().expect("response was an error");
        let response = match response {
            ResponseData::GetAttributes(response) => response,
            _ => panic!("unexpected response"),
        };
        let download_url = response
            .download_url
            .as_ref()
            .expect("missing download url");

        let mut response = client
            .client
            .get(download_url.as_str())
            .send()
            .await
            .expect("failed to send")
            .error_for_status()
            .expect("invalid status");
        let mut cipher = Aes128Ctr128BE::new(
            &file_key.key.to_be_bytes().into(),
            &file_key.iv.to_be_bytes().into(),
        );

        let mut output = Vec::new();
        let mut validator = FileValidator::new(file_key.clone());
        while let Some(chunk) = response.chunk().await.expect("failed to get chunk") {
            let old_len = output.len();
            output.extend(&chunk);
            cipher.apply_keystream(&mut output[old_len..]);
        }
        assert!(output == TEST_FILE_BYTES);

        validator.feed(&output);
        validator.finish().expect("validation failed");
    }
}
