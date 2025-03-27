mod reader;
mod util;

pub use self::reader::FileDownloadReader;
pub use self::util::ArcError;
use crate::Command;
use crate::Error;
use crate::FetchNodesResponse;
use crate::FileKey;
use crate::GetAttributesResponse;
use crate::ResponseData;
use std::future::Future;
use std::pin::Pin;
// use std::sync::Arc;
// use std::sync::Mutex;
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::io::StreamReader;

/// A client
#[derive(Debug, Clone)]
pub struct Client {
    /// The low-level api client
    pub client: crate::Client,
    // /// Client state
    // state: Arc<Mutex<State>>,
}

impl Client {
    /// Make a new client
    pub fn new() -> Self {
        Self {
            client: crate::Client::new(),
            /*
            state: Arc::new(Mutex::new(State {
                buffered_commands: Vec::with_capacity(4),
                buffered_tx: Vec::with_capacity(4),
            })),
            */
        }
    }

    /*
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
    */

    /// Get attributes for a file.
    pub fn get_attributes(
        &self,
        builder: GetAttributesBuilder,
    ) -> impl Future<Output = Result<GetAttributesResponse, Error>> {
        let command = Command::GetAttributes {
            public_file_id: builder.public_file_id,
            node_id: builder.node_id,
            include_download_url: if builder.include_download_url {
                Some(1)
            } else {
                None
            },
        };

        async move {
            let commands = [command];

            let response = self
                .client
                .execute_commands(&commands, builder.reference_node_id.as_deref());

            let response = match response.await?.swap_remove(0).into_result()? {
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
    pub async fn fetch_nodes(
        &self,
        node_id: Option<&str>,
        recursive: bool,
    ) -> Result<FetchNodesResponse, Error> {
        let command = Command::FetchNodes {
            c: 1,
            recursive: u8::from(recursive),
        };
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
    ) -> Result<FileDownloadReader<Pin<Box<dyn AsyncRead + Send + Sync>>>, Error> {
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
        let stream_reader =
            Box::into_pin(Box::new(stream_reader) as Box<dyn AsyncRead + Send + Sync>);

        let reader = FileDownloadReader::new(stream_reader, file_key, false);

        Ok(reader)
    }

    /// Download a file and verify its integrity.
    ///
    /// # Returns
    /// Returns a reader.
    pub async fn download_file(
        &self,
        file_key: &FileKey,
        url: &str,
    ) -> Result<FileDownloadReader<Pin<Box<dyn AsyncRead + Send + Sync>>>, Error> {
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
        let stream_reader =
            Box::into_pin(Box::new(stream_reader) as Box<dyn AsyncRead + Send + Sync>);

        let reader = FileDownloadReader::new(stream_reader, file_key, true);

        Ok(reader)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/*
/// The client state
#[derive(Debug)]
struct State {
    buffered_commands: Vec<Command>,
    buffered_tx: Vec<tokio::sync::oneshot::Sender<Result<ResponseData, Error>>>,
}
*/

/// A builder for a get_attributes call.
#[derive(Debug)]
pub struct GetAttributesBuilder {
    /// The public id of the node.
    ///
    /// Mutually exclusive with `node_id`.
    pub public_file_id: Option<String>,
    /// The node id.
    ///
    /// Mutually exclusive with `public_file_id`.
    pub node_id: Option<String>,
    /// Whether this should include the download url
    pub include_download_url: bool,

    /// The reference node id.
    pub reference_node_id: Option<String>,
}

impl GetAttributesBuilder {
    /// Make a new builder.
    pub fn new() -> Self {
        Self {
            public_file_id: None,
            node_id: None,
            include_download_url: false,
            reference_node_id: None,
        }
    }

    /// Set the public file id.
    ///
    /// Mutually exclusive with `node_id`.
    pub fn public_file_id(&mut self, value: impl Into<String>) -> &mut Self {
        self.public_file_id = Some(value.into());
        self
    }

    /// Set the node id.
    ///
    /// Mutually exclusive with `public_file_id`.
    pub fn node_id(&mut self, value: impl Into<String>) -> &mut Self {
        self.node_id = Some(value.into());
        self
    }

    /// Set the include_download_url field.
    pub fn include_download_url(&mut self, value: bool) -> &mut Self {
        self.include_download_url = value;
        self
    }

    /// Set the reference node id.
    pub fn reference_node_id(&mut self, value: impl Into<String>) -> &mut Self {
        self.reference_node_id = Some(value.into());
        self
    }
}

impl Default for GetAttributesBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::FolderKey;
    use crate::test::*;
    use tokio::io::AsyncReadExt;

    /*
    #[tokio::test]
    async fn get_attributes() {
        let client = Client::new();
        let get_attributes_1_future = client.get_attributes(TEST_FILE_ID, false);
        let get_attributes_2_future = client.get_attributes(TEST_FILE_ID, true);

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
    */

    #[tokio::test]
    async fn fetch_nodes() {
        let folder_key = FolderKey(TEST_FOLDER_KEY_DECODED);

        let client = Client::new();
        let response = client
            .fetch_nodes(Some(TEST_FOLDER_ID), true)
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
        let mut builder = GetAttributesBuilder::new();
        builder
            .include_download_url(true)
            .public_file_id(TEST_FILE_ID);
        let attributes = client
            .get_attributes(builder)
            .await
            .expect("failed to get attributes");
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
        {
            let mut builder = GetAttributesBuilder::new();
            builder
                .include_download_url(true)
                .public_file_id(TEST_FILE_ID);
            let attributes = client
                .get_attributes(builder)
                .await
                .expect("failed to get attributes");
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

        {
            let mut builder = GetAttributesBuilder::new();
            builder
                .include_download_url(true)
                .public_file_id(TEST_FILE_ID);
            let attributes = client
                .get_attributes(builder)
                .await
                .expect("failed to get attributes");
            let url = attributes.download_url.expect("missing download url");
            let mut reader = client
                .download_file(&file_key, url.as_str())
                .await
                .expect("failed to get download stream");
            let mut file = Vec::new();
            reader.read_to_end(&mut file).await.unwrap();

            assert!(file == TEST_FILE_BYTES);
        }
    }
}
