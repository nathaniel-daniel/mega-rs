mod types;

pub use self::types::Command;
pub use self::types::FileKey;
pub use self::types::FileKeyParseError;
pub use self::types::FolderKey;
pub use self::types::FolderKeyParseError;
pub use self::types::Response;
pub use self::types::ResponseData;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use url::Url;

/// The library error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A reqwest Error
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    /// A Url Error
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

/// A client
#[derive(Debug)]
pub struct Client {
    /// The inner http client
    pub client: reqwest::Client,

    /// The sequence id
    pub sequence_id: Arc<AtomicU64>,
}

impl Client {
    /// Make a new client
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            sequence_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Execute a series of commands.
    pub async fn execute_commands(
        &self,
        commands: &[Command],
        node: Option<&str>,
    ) -> Result<Vec<Response<ResponseData>>, Error> {
        let id = self.sequence_id.fetch_add(1, Ordering::Relaxed);
        let mut url =
            Url::parse_with_params("https://g.api.mega.co.nz/cs", &[("id", id.to_string())])?;
        {
            let mut query_pairs = url.query_pairs_mut();
            if let Some(node) = node {
                query_pairs.append_pair("n", node);
            }
        }
        let response = self
            .client
            .post(url)
            .json(commands)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // const TEST_FILE: &str =
    //    "https://mega.nz/file/7glwEQBT#Fy9cwPpCmuaVdEkW19qwBLaiMeyufB1kseqisOAxfi8";
    const TEST_FILE_KEY: &str = "Fy9cwPpCmuaVdEkW19qwBLaiMeyufB1kseqisOAxfi8";
    const TEST_FILE_ID: &str = "7glwEQBT";

    // const TEST_FOLDER: &str = "https://mega.nz/folder/MWsm3aBL#xsXXTpoYEFDRQdeHPDrv7A";
    const TEST_FOLDER_KEY: &str = "xsXXTpoYEFDRQdeHPDrv7A";
    const TEST_FOLDER_ID: &str = "MWsm3aBL";

    const TEST_FILE_KEY_DECODED: &[u8; 16] = &[
        161, 141, 109, 44, 84, 62, 135, 130, 36, 158, 235, 166, 55, 235, 206, 43,
    ];
    const TEST_FOLDER_KEY_DECODED: &[u8; 16] = &[
        198, 197, 215, 78, 154, 24, 16, 80, 209, 65, 215, 135, 60, 58, 239, 236,
    ];

    #[test]
    fn parse_file_key() {
        let file_key: FileKey = TEST_FILE_KEY.parse().expect("failed to parse file key");
        assert!(&file_key.0 == TEST_FILE_KEY_DECODED);
    }

    #[test]
    fn parse_folder_key() {
        let folder_key: FolderKey = TEST_FOLDER_KEY.parse().expect("failed to parse folder key");
        assert!(&folder_key.0 == TEST_FOLDER_KEY_DECODED);
    }

    #[tokio::test]
    async fn execute_empty_commands() {
        let client = Client::new();
        let response = client
            .execute_commands(&[], None)
            .await
            .expect("failed to execute commands");
        assert!(response.is_empty());
    }

    #[tokio::test]
    async fn execute_get_attributes_command() {
        let client = Client::new();
        let commands = vec![Command::GetAttributes {
            file_id: TEST_FILE_ID.into(),
            include_download_url: None,
        }];
        let mut response = client
            .execute_commands(&commands, None)
            .await
            .expect("failed to execute commands");
        assert!(response.len() == 1);
        let response = response.swap_remove(0);
        let response = response.unwrap();
        let response = match response {
            ResponseData::GetAttributes(response) => response,
            _ => panic!("unexpected response"),
        };
        assert!(response.download_url.is_none());
        let file_attributes = response
            .decode_attributes(TEST_FILE_KEY_DECODED)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "Doxygen_docs.zip");

        let commands = vec![Command::GetAttributes {
            file_id: TEST_FILE_ID.into(),
            include_download_url: Some(1),
        }];
        let mut response = client
            .execute_commands(&commands, None)
            .await
            .expect("failed to execute commands");
        assert!(response.len() == 1);
        let response = response.swap_remove(0);
        let response = response.unwrap();
        let response = match response {
            ResponseData::GetAttributes(response) => response,
            _ => panic!("unexpected response"),
        };
        assert!(response.download_url.is_some());
        let file_attributes = response
            .decode_attributes(TEST_FILE_KEY_DECODED)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "Doxygen_docs.zip");
    }

    #[tokio::test]
    async fn execute_fetch_nodes_command() {
        let folder_key = FolderKey(*TEST_FOLDER_KEY_DECODED);

        let client = Client::new();
        let commands = vec![Command::FetchNodes { c: 1, r: 1 }];
        let mut response = client
            .execute_commands(&commands, Some(TEST_FOLDER_ID))
            .await
            .expect("failed to execute commands");
        assert!(response.len() == 1);
        let response = response.swap_remove(0);
        let response = response.unwrap();
        let response = match response {
            ResponseData::FetchNodes(response) => response,
            _ => panic!("unexpected response"),
        };
        assert!(response.files.len() == 2);
        let file_attributes = response.files[0]
            .decode_attributes(&folder_key)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "test");
    }
}
