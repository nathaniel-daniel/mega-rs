/// A command
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "a")]
pub enum Command {
    /// Get the attributes of a file
    #[serde(rename = "g")]
    GetAttributes {
        /// The id of the file
        #[serde(rename = "p")]
        file_id: String,

        ///  Set to Some(1) to include the download url in the response.
        #[serde(rename = "g")]
        include_download_url: Option<u8>,
    },
}
