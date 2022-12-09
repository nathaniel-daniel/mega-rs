mod client;
#[cfg(feature = "easy")]
mod easy;
mod types;

pub use self::client::Client;
#[cfg(feature = "easy")]
pub use self::easy::Client as EasyClient;
pub use self::types::Command;
pub use self::types::ErrorCode;
pub use self::types::FetchNodesResponse;
pub use self::types::FileKey;
pub use self::types::FileKeyParseError;
pub use self::types::FolderKey;
pub use self::types::FolderKeyParseError;
pub use self::types::GetAttributesResponse;
pub use self::types::Response;
pub use self::types::ResponseData;

/// The library error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A reqwest Error
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    /// A Url Error
    #[error(transparent)]
    Url(#[from] url::ParseError),

    /// The returned number of responses did not match what was expected
    #[error("expected '{expected}' responses, but got '{actual}'")]
    ResponseLengthMismatch { expected: usize, actual: usize },

    /// There was an api error
    #[error("api error")]
    ApiError(#[from] ErrorCode),

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

#[cfg(test)]
mod test {
    use super::*;
    use cbc::cipher::KeyIvInit;
    use cbc::cipher::StreamCipher;

    type Aes128Ctr128BE = ctr::Ctr128BE<aes::Aes128>;

    // const TEST_FILE: &str =
    //    "https://mega.nz/file/7glwEQBT#Fy9cwPpCmuaVdEkW19qwBLaiMeyufB1kseqisOAxfi8";
    pub const TEST_FILE_KEY: &str = "Fy9cwPpCmuaVdEkW19qwBLaiMeyufB1kseqisOAxfi8";
    pub const TEST_FILE_ID: &str = "7glwEQBT";

    // const TEST_FOLDER: &str = "https://mega.nz/folder/MWsm3aBL#xsXXTpoYEFDRQdeHPDrv7A";
    pub const TEST_FOLDER_KEY: &str = "xsXXTpoYEFDRQdeHPDrv7A";
    pub const TEST_FOLDER_ID: &str = "MWsm3aBL";

    pub const TEST_FILE_KEY_KEY_DECODED: u128 = u128::from_ne_bytes([
        161, 141, 109, 44, 84, 62, 135, 130, 36, 158, 235, 166, 55, 235, 206, 43,
    ]);
    pub const TEST_FILE_KEY_IV_DECODED: u128 =
        u128::from_ne_bytes([182, 162, 49, 236, 174, 124, 29, 100, 0, 0, 0, 0, 0, 0, 0, 0]);
    pub const TEST_FILE_META_MAC_DECODED: u64 =
        u64::from_ne_bytes([177, 234, 162, 176, 224, 49, 126, 47]);
    pub const TEST_FOLDER_KEY_DECODED: u128 = u128::from_ne_bytes([
        198, 197, 215, 78, 154, 24, 16, 80, 209, 65, 215, 135, 60, 58, 239, 236,
    ]);

    const TEST_FILE_BYTES: &[u8] = include_bytes!("../test_data/Doxygen_docs.zip");

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

    #[test]
    fn chunk_iter() {
        let mut iter = ChunkIter::new();
        assert!(iter.next() == Some((0, 128 * 1024)));
        assert!(iter.next() == Some((128 * 1024, 128 * 2 * 1024)));
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
    async fn download_file() {
        let file_key = FileKey {
            key: TEST_FILE_KEY_KEY_DECODED,
            iv: TEST_FILE_KEY_IV_DECODED,
            meta_mac: TEST_FILE_META_MAC_DECODED,
        };

        let client = Client::new();
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
        let download_url = response
            .download_url
            .as_ref()
            .expect("missing download url");
        {
            let mut response = client
                .client
                .get(download_url.as_str())
                .send()
                .await
                .expect("failed to send")
                .error_for_status()
                .expect("invalid status");

            let mut cipher = Aes128Ctr128BE::new(
                &file_key.key.to_ne_bytes().into(),
                &file_key.iv.to_ne_bytes().into(),
            );
            let mut chunk_iter = ChunkIter::new();
            let mut buffer = Vec::with_capacity(1024 * 1024);
            let (_chunk_offset, chunk_size) = chunk_iter.next().unwrap();
            let mut output: Vec<u8> = Vec::with_capacity(1024 * 1024);
            while let Some(chunk) = response.chunk().await.expect("failed to get chunk") {
                let mut chunk = chunk.as_ref();

                while buffer.len() + chunk.len() >= chunk_size.try_into().unwrap() {
                    let to_read = usize::try_from(chunk_size).unwrap() - buffer.len();
                    buffer.extend(&chunk[..to_read]);
                    cipher.apply_keystream(&mut buffer);
                    output.extend(&buffer);
                    buffer.clear();
                    chunk = &chunk[to_read..];
                }
                buffer.extend(chunk);
            }
            if !buffer.is_empty() {
                cipher.apply_keystream(&mut buffer);
                output.extend(&buffer);
                buffer.clear();
            }

            assert!(output == TEST_FILE_BYTES);
        }
    }
}
