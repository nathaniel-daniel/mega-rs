use crate::Command;
use crate::Error;
use crate::Response;
use crate::ResponseData;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use url::Url;

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
        let mut url = Url::parse_with_params(
            "https://g.api.mega.co.nz/cs",
            &[("id", itoa::Buffer::new().format(id))],
        )?;
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
    use crate::test::*;
    use crate::*;

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
        let response = response.into_result().expect("response was an error");
        let response = match response {
            ResponseData::GetAttributes(response) => response,
            _ => panic!("unexpected response"),
        };
        assert!(response.download_url.is_none());
        let file_attributes = response
            .decode_attributes(TEST_FILE_KEY_KEY_DECODED)
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
        let response = response.into_result().expect("response was an error");
        let response = match response {
            ResponseData::GetAttributes(response) => response,
            _ => panic!("unexpected response"),
        };
        assert!(response.download_url.is_some());
        let file_attributes = response
            .decode_attributes(TEST_FILE_KEY_KEY_DECODED)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "Doxygen_docs.zip");
    }

    #[tokio::test]
    async fn execute_fetch_nodes_command() {
        let folder_key = FolderKey(TEST_FOLDER_KEY_DECODED);

        let client = Client::new();
        let commands = vec![Command::FetchNodes { c: 1, r: 1 }];
        let mut response = client
            .execute_commands(&commands, Some(TEST_FOLDER_ID))
            .await
            .expect("failed to execute commands");
        assert!(response.len() == 1);
        let response = response.swap_remove(0);
        let response = response.into_result().expect("response was an error");
        let response = match response {
            ResponseData::FetchNodes(response) => response,
            _ => panic!("unexpected response"),
        };
        assert!(response.files.len() == 3);
        let file_attributes = response.files[0]
            .decode_attributes(&folder_key)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "test");

        let file_attributes = dbg!(&response.files[1])
            .decode_attributes(&folder_key)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "test.txt");

        let file_attributes = dbg!(&response.files[2])
            .decode_attributes(&folder_key)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "testfolder");
    }
}
