mod util;

pub use self::util::ArcError;
use crate::Command;
use crate::Error;
use crate::FetchNodesResponse;
use crate::FileKey;
use crate::GetAttributesResponse;
use crate::ResponseData;
use cbc::cipher::BlockEncryptMut;
use cbc::cipher::KeyIvInit;
use cbc::cipher::StreamCipher;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;
use std::task::ready;
use tokio::io::AsyncRead;
use tokio::io::ReadBuf;
use tokio_stream::StreamExt;
use tokio_util::io::StreamReader;

type Aes128Ctr128BE = ctr::Ctr128BE<aes::Aes128>;
type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

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

pin_project! {
    struct DownloadNoValidateReader<R> {
        #[pin]
        reader: R,
        cipher: Aes128Ctr128BE,
    }
}

impl<R> DownloadNoValidateReader<R> {
    fn new(reader: R, file_key: &FileKey) -> Self {
        let cipher = Aes128Ctr128BE::new(
            &file_key.key.to_be_bytes().into(),
            &file_key.iv.to_be_bytes().into(),
        );

        Self { reader, cipher }
    }
}

impl<R> AsyncRead for DownloadNoValidateReader<R>
where
    R: AsyncRead,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // See: https://users.rust-lang.org/t/blocking-permit/36865/5
        const MAX_LEN: usize = 64 * 1024;

        let this = self.as_mut().project();

        let mut unfilled_buf = buf.take(MAX_LEN);

        let result = ready!(this.reader.poll_read(cx, &mut unfilled_buf));
        result?;

        let new_bytes = unfilled_buf.filled_mut();
        this.cipher.apply_keystream(new_bytes);
        let new_bytes_len = new_bytes.len();
        buf.advance(new_bytes_len);

        Poll::Ready(Ok(()))
    }
}

/// An iterator over chunks
struct ChunkIter {
    /// The offset into the file
    offset: u64,
    delta: u64,
}

impl ChunkIter {
    fn new() -> Self {
        Self {
            delta: 0,
            offset: 0,
        }
    }
}

impl Iterator for ChunkIter {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<Self::Item> {
        self.delta += 128 * 1024;
        self.delta = std::cmp::min(self.delta, 1024 * 1024);

        let old_offset = self.offset;
        self.offset += self.delta;

        Some((old_offset, self.delta))
    }
}

pin_project! {
    struct DownloadValidateReader<R> {
        #[pin]
        reader: R,
        cipher: Aes128Ctr128BE,

        file_key: FileKey,
        chunk_iter: ChunkIter,
        left_in_chunk: usize,
        file_mac: u128,
        chunk_mac: u128,
        buffer: Vec<u8>,
    }
}

impl<R> DownloadValidateReader<R> {
    fn new(reader: R, file_key: &FileKey) -> Self {
        const MAX_CHUNK_SIZE: usize = 128 * 8 * 1024;

        let cipher = Aes128Ctr128BE::new(
            &file_key.key.to_be_bytes().into(),
            &file_key.iv.to_be_bytes().into(),
        );
        let mut chunk_iter = ChunkIter::new();
        // ChunkIter is infinite.
        let (_, left_in_chunk) = chunk_iter.next().unwrap();
        // This can only fail when a usize is a u16.
        let left_in_chunk = usize::try_from(left_in_chunk).unwrap();
        let chunk_mac = create_chunk_mac(file_key);
        let buffer = Vec::with_capacity(MAX_CHUNK_SIZE);

        Self {
            reader,
            cipher,

            file_key: file_key.clone(),
            chunk_iter,
            left_in_chunk,
            file_mac: 0,
            chunk_mac,
            buffer,
        }
    }
}

impl<R> AsyncRead for DownloadValidateReader<R>
where
    R: AsyncRead,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // See: https://users.rust-lang.org/t/blocking-permit/36865/5
        const MAX_LEN: usize = 64 * 1024;

        let this = self.as_mut().project();

        // Limit max chunk processed at a time to avoid blocking.
        let mut unfilled_buf = buf.take(MAX_LEN);

        let result = ready!(this.reader.poll_read(cx, &mut unfilled_buf));
        result?;

        let new_bytes = unfilled_buf.filled_mut();
        this.cipher.apply_keystream(new_bytes);
        let new_bytes_len = new_bytes.len();

        if new_bytes_len == 0 {
            *this.file_mac ^= *this.chunk_mac;
            let mut file_mac_bytes = this.file_mac.to_be_bytes();
            aes_cbc_encrypt_u128(this.file_key.key, &mut file_mac_bytes);
            *this.file_mac = u128::from_be_bytes(file_mac_bytes);

            let file_mac_bytes = this.file_mac.to_be_bytes();
            let file_mac_u32_0 = u32::from_be_bytes(file_mac_bytes[..4].try_into().unwrap());
            let file_mac_u32_1 = u32::from_be_bytes(file_mac_bytes[4..8].try_into().unwrap());
            let file_mac_u32_2 = u32::from_be_bytes(file_mac_bytes[8..12].try_into().unwrap());
            let file_mac_u32_3 = u32::from_be_bytes(file_mac_bytes[12..].try_into().unwrap());

            let final_file_mac_u32_0 = file_mac_u32_0 ^ file_mac_u32_1;
            let final_file_mac_u32_1 = file_mac_u32_2 ^ file_mac_u32_3;

            let mut final_file_mac_bytes = [0; 8];
            final_file_mac_bytes[..4].copy_from_slice(&final_file_mac_u32_0.to_be_bytes());
            final_file_mac_bytes[4..].copy_from_slice(&final_file_mac_u32_1.to_be_bytes());
            let final_file_mac = u64::from_be_bytes(final_file_mac_bytes);

            if final_file_mac != this.file_key.meta_mac {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "mac mismatch",
                )));
            }
        } else {
            this.buffer.extend(&*new_bytes);

            let mut buffer_start = 0;
            while this.buffer[buffer_start..].len() >= 16 {
                let mut len = std::cmp::min(*this.left_in_chunk, this.buffer[buffer_start..].len());
                len -= len % 16;

                let mut chunk_iter = this.buffer[buffer_start..buffer_start + len].chunks_exact(16);
                for chunk in &mut chunk_iter {
                    let block: [u8; 16] = chunk
                        .try_into()
                        .expect("chunk should always be a multiple of 16");
                    *this.chunk_mac ^= u128::from_be_bytes(block);
                    let mut chunk_mac_bytes = this.chunk_mac.to_be_bytes();
                    aes_cbc_encrypt_u128(this.file_key.key, &mut chunk_mac_bytes);
                    *this.chunk_mac = u128::from_be_bytes(chunk_mac_bytes);
                }
                buffer_start += len;

                *this.left_in_chunk -= len;
                if *this.left_in_chunk == 0 {
                    *this.file_mac ^= *this.chunk_mac;
                    let mut file_mac_bytes = this.file_mac.to_be_bytes();
                    aes_cbc_encrypt_u128(this.file_key.key, &mut file_mac_bytes);
                    *this.file_mac = u128::from_be_bytes(file_mac_bytes);

                    *this.chunk_mac = create_chunk_mac(this.file_key);

                    // ChunkIter is infinite.
                    let (_, left_in_chunk) = this.chunk_iter.next().unwrap();
                    // This can only fail when a usize is a u16.
                    *this.left_in_chunk = usize::try_from(left_in_chunk).unwrap();
                }
            }
            let mut remainder_copy = [0; 16];
            // dbg!(&this.buffer[buffer_start..]);
            let remainder_len = this.buffer[buffer_start..].len();
            remainder_copy[..remainder_len].copy_from_slice(&this.buffer[buffer_start..]);
            this.buffer.clear();
            if remainder_len != 0 {
                this.buffer.extend(&remainder_copy[..remainder_len]);
            }

            buf.advance(new_bytes_len);
        }

        Poll::Ready(Ok(()))
    }
}

fn create_chunk_mac(file_key: &FileKey) -> u128 {
    let mut chunk_mac_bytes = [0; 16];
    let iv_bytes = file_key.iv.to_be_bytes();
    chunk_mac_bytes[..8].copy_from_slice(&iv_bytes[..8]);
    chunk_mac_bytes[8..].copy_from_slice(&iv_bytes[..8]);
    u128::from_be_bytes(chunk_mac_bytes)
}

fn aes_cbc_encrypt_u128(key: u128, data: &mut [u8; 16]) {
    let mut cipher = Aes128CbcEnc::new(&key.to_be_bytes().into(), &[0; 16].into());
    cipher.encrypt_block_mut((data).into());
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::FolderKey;
    use crate::test::*;

    #[test]
    #[expect(clippy::erasing_op, clippy::identity_op)]
    fn chunk_iter() {
        let mut iter = ChunkIter::new();
        assert!(iter.next() == Some((128 * 0 * 2014, 128 * 1 * 1024)));
        assert!(iter.next() == Some((128 * 1 * 1024, 128 * 2 * 1024)));
        assert!(iter.next() == Some((128 * 3 * 1024, 128 * 3 * 1024)));
        assert!(iter.next() == Some((128 * 6 * 1024, 128 * 4 * 1024)));
        assert!(iter.next() == Some((128 * 10 * 1024, 128 * 5 * 1024)));
        assert!(iter.next() == Some((128 * 15 * 1024, 128 * 6 * 1024)));
        assert!(iter.next() == Some((128 * 21 * 1024, 128 * 7 * 1024)));
        assert!(iter.next() == Some((128 * 28 * 1024, 128 * 8 * 1024)));
        assert!(iter.next() == Some((128 * 36 * 1024, 128 * 8 * 1024)));
        assert!(iter.next() == Some((128 * 44 * 1024, 128 * 8 * 1024)));
    }

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
