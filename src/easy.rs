mod reader;
mod util;

use self::reader::DownloadNoValidateReader;
use self::reader::DownloadValidateReader;
pub use self::util::ArcError;
use crate::Command;
use crate::Error;
use crate::FetchNodesResponse;
use crate::FileKey;
use crate::GetAttributesResponse;
use crate::ResponseData;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::io::StreamReader;

/// A client
#[derive(Debug, Clone)]
pub struct Client {
    /// The low-level api client
    pub client: crate::Client,

    /// Client state
    state: Arc<Mutex<State>>,
}

impl Client {
    /// Make a new client
    pub fn new() -> Self {
        Self {
            client: crate::Client::new(),
            state: Arc::new(Mutex::new(State {
                buffered_commands: Vec::with_capacity(4),
                buffered_tx: Vec::with_capacity(4),
            })),
        }
    }

    /// Queue a command to be sent
    fn queue_command(
        &self,
        command: Command,
    ) -> tokio::sync::oneshot::Receiver<Result<ResponseData, Error>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut state = self.state.lock().unwrap();
            state.buffered_commands.push(command);
            state.buffered_tx.push(tx);
        }
        rx
    }

    /// Send all buffered commands
    pub fn send_commands(&self) {
        let (commands, tx) = {
            let mut state = self.state.lock().unwrap();
            if state.buffered_commands.is_empty() {
                return;
            }

            let mut commands = Vec::with_capacity(4);
            std::mem::swap(&mut commands, &mut state.buffered_commands);

            let mut tx = Vec::with_capacity(4);
            std::mem::swap(&mut tx, &mut state.buffered_tx);

            (commands, tx)
        };

        let self_clone = self.clone();
        tokio::spawn(async move {
            let response = self_clone
                .client
                .execute_commands(&commands, None)
                .await
                .map_err(ArcError::new);
            match response {
                Ok(mut response) => {
                    for tx in tx.into_iter().rev() {
                        // The low-level api client ensures that the number of returned responses matches the number of input commands.
                        let response = response.pop().unwrap();
                        let response = response.into_result().map_err(Error::from);
                        let _ = tx.send(response).is_ok();
                    }
                }
                Err(error) => {
                    for tx in tx {
                        let _ = tx.send(Err(Error::BatchSend(error.clone()))).is_ok();
                    }
                }
            };
        });
    }

    /// Get attributes for a file.
    pub fn get_attributes(
        &self,
        file_id: &str,
        include_download_url: bool,
    ) -> impl Future<Output = Result<GetAttributesResponse, Error>> {
        let rx = self.queue_command(Command::GetAttributes {
            file_id: file_id.to_string(),
            include_download_url: if include_download_url { Some(1) } else { None },
        });

        async {
            let response = rx.await.map_err(|_e| Error::NoResponse)??;
            let response = match response {
                ResponseData::GetAttributes(response) => response,
                _ => {
                    return Err(Error::UnexpectedResponseDataType);
                }
            };

            Ok(response)
        }
    }

    /// Get the nodes for a folder node.
    ///
    /// This bypasses the command buffering system as it is more efficient for Mega's servers to process this alone.
    pub async fn fetch_nodes(&self, node_id: Option<&str>) -> Result<FetchNodesResponse, Error> {
        let command = Command::FetchNodes { c: 1, r: 1 };
        let mut response = self
            .client
            .execute_commands(std::slice::from_ref(&command), node_id)
            .await?;

        // The low-level api client ensures that the number of returned responses matches the number of input commands.
        let response = response.pop().unwrap();
        let response = response.into_result().map_err(Error::from)?;
        let response = match response {
            ResponseData::FetchNodes(response) => response,
            _ => {
                return Err(Error::UnexpectedResponseDataType);
            }
        };

        Ok(response)
    }

    /// Download a file without verifying its integrity.
    ///
    /// # Returns
    /// Returns a reader.
    pub async fn download_file_no_verify(
        &self,
        file_key: &FileKey,
        url: &str,
    ) -> Result<impl AsyncRead, Error> {
        let response = self
            .client
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?;

        let stream_reader = StreamReader::new(
            response
                .bytes_stream()
                .map(|result| result.map_err(std::io::Error::other)),
        );

        let reader = DownloadNoValidateReader::new(stream_reader, file_key);

        Ok(reader)
    }

    /// Download a file and verify its integrity.
    ///
    /// Note that this verification is not perfect.
    /// Corruption of the last 0-15 bytes of the file will not be detected.
    /// # Returns
    /// Returns a reader.
    pub async fn download_file(
        &self,
        file_key: &FileKey,
        url: &str,
    ) -> Result<impl AsyncRead, Error> {
        let response = self
            .client
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?;

        let stream_reader = StreamReader::new(
            response
                .bytes_stream()
                .map(|result| result.map_err(std::io::Error::other)),
        );

        let reader = DownloadValidateReader::new(stream_reader, file_key);

        Ok(reader)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// The client state
#[derive(Debug)]
struct State {
    buffered_commands: Vec<Command>,
    buffered_tx: Vec<tokio::sync::oneshot::Sender<Result<ResponseData, Error>>>,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::FolderKey;
    use crate::test::*;

    #[tokio::test]
    async fn get_attributes() {
        let client = Client::new();
        let get_attributes_1_future = client.get_attributes(TEST_FILE_ID, false);
        let get_attributes_2_future = client.get_attributes(TEST_FILE_ID, true);
        client.send_commands();

        let attributes_1 = get_attributes_1_future
            .await
            .expect("failed to get attributes");
        assert!(attributes_1.download_url.is_none());
        let attributes_2 = get_attributes_2_future
            .await
            .expect("failed to get attributes");
        let file_attributes = attributes_1
            .decode_attributes(TEST_FILE_KEY_KEY_DECODED)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "Doxygen_docs.zip");
        assert!(attributes_2.download_url.is_some());
        let file_attributes = attributes_2
            .decode_attributes(TEST_FILE_KEY_KEY_DECODED)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "Doxygen_docs.zip");
    }

    #[tokio::test]
    async fn fetch_nodes() {
        let folder_key = FolderKey(TEST_FOLDER_KEY_DECODED);

        let client = Client::new();
        let response = client
            .fetch_nodes(Some(TEST_FOLDER_ID))
            .await
            .expect("failed to fetch nodes");
        assert!(response.files.len() == 3);
        let file_attributes = response
            .files
            .iter()
            .find(|file| file.id == "oLkVhYqA")
            .expect("failed to locate file")
            .decode_attributes(&folder_key)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "test");

        let file_attributes = response
            .files
            .iter()
            .find(|file| file.id == "kalwUahb")
            .expect("failed to locate file")
            .decode_attributes(&folder_key)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "test.txt");

        let file_attributes = &response
            .files
            .iter()
            .find(|file| file.id == "IGlBlD6K")
            .expect("failed to locate file")
            .decode_attributes(&folder_key)
            .expect("failed to decode attributes");
        assert!(file_attributes.name == "testfolder");
    }

    #[tokio::test]
    async fn download_file_no_verify() {
        let file_key = FileKey {
            key: TEST_FILE_KEY_KEY_DECODED,
            iv: TEST_FILE_KEY_IV_DECODED,
            meta_mac: TEST_FILE_META_MAC_DECODED,
        };

        let client = Client::new();
        let attributes = client.get_attributes(TEST_FILE_ID, true);
        client.send_commands();
        let attributes = attributes.await.expect("failed to get attributes");
        let url = attributes.download_url.expect("missing download url");
        let mut reader = client
            .download_file_no_verify(&file_key, url.as_str())
            .await
            .expect("failed to get download stream");
        let mut file = Vec::with_capacity(1024 * 1024);
        tokio::io::copy(&mut reader, &mut file)
            .await
            .expect("failed to copy");

        assert!(file == TEST_FILE_BYTES);
    }

    #[tokio::test]
    async fn download_file_verify() {
        let file_key = FileKey {
            key: TEST_FILE_KEY_KEY_DECODED,
            iv: TEST_FILE_KEY_IV_DECODED,
            meta_mac: TEST_FILE_META_MAC_DECODED,
        };

        let client = Client::new();
        let attributes = client.get_attributes(TEST_FILE_ID, true);
        client.send_commands();
        let attributes = attributes.await.expect("failed to get attributes");
        let url = attributes.download_url.expect("missing download url");
        let mut reader = client
            .download_file(&file_key, url.as_str())
            .await
            .expect("failed to get download stream");
        let mut file = Vec::with_capacity(1024 * 1024);
        tokio::io::copy(&mut reader, &mut file)
            .await
            .expect("failed to copy");

        assert!(file == TEST_FILE_BYTES);
    }
}
