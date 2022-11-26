mod types;

pub use self::types::Command;
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
            sequence_id: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Execute a series of commands.
    pub async fn execute_commands(
        &self,
        commands: &[Command],
    ) -> Result<Vec<Response<ResponseData>>, Error> {
        let id = self.sequence_id.fetch_add(1, Ordering::Relaxed);
        let url = Url::parse_with_params("https://g.api.mega.co.nz/cs", &[("id", id.to_string())])?;
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
    const TEST_FILE_ID: &str = "7glwEQBT";

    #[tokio::test]
    async fn execute_empty_commands() {
        let client = Client::new();
        let response = client
            .execute_commands(&[])
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
            .execute_commands(&commands)
            .await
            .expect("failed to execute commands");
        assert!(response.len() == 1);
        let response = response.swap_remove(0);
        let response = response.unwrap();
        let response = match response {
            ResponseData::GetAttributes(response) => response,
            // _ => panic!("unexpected response"),
        };
        assert!(response.download_url.is_none());

        let commands = vec![Command::GetAttributes {
            file_id: TEST_FILE_ID.into(),
            include_download_url: Some(1),
        }];
        let mut response = client
            .execute_commands(&commands)
            .await
            .expect("failed to execute commands");
        assert!(response.len() == 1);
        let response = response.swap_remove(0);
        let response = response.unwrap();
        let response = match response {
            ResponseData::GetAttributes(response) => response,
            // _ => panic!("unexpected response"),
        };
        assert!(response.download_url.is_some());
    }
}
