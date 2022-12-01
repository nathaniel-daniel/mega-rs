mod client;
mod types;

pub use self::client::Client;
pub use self::types::Command;
pub use self::types::FileKey;
pub use self::types::FileKeyParseError;
pub use self::types::FolderKey;
pub use self::types::FolderKeyParseError;
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

    const TEST_FILE_BYTES: &[u8] = include_bytes!("../test_data/Doxygen_Docs.zip");

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
            let response = client
                .client
                .get(download_url.as_str())
                .send()
                .await
                .expect("failed to send")
                .error_for_status()
                .expect("invalid status");
            let mut bytes = response
                .bytes()
                .await
                .expect("failed to get bytes")
                .to_vec();

            let mut cipher = Aes128Ctr128BE::new(
                &file_key.key.to_ne_bytes().into(),
                &file_key.iv.to_ne_bytes().into(),
            );
            cipher.apply_keystream(&mut bytes);

            assert!(bytes == TEST_FILE_BYTES);
        }
    }
}
