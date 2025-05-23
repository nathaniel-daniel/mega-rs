/// A command
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "a")]
pub enum Command {
    /// Get the attributes of a node
    #[serde(rename = "g")]
    GetAttributes {
        /// The public id of the node
        #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
        public_node_id: Option<String>,

        /// The id of the node
        #[serde(rename = "n")]
        node_id: Option<String>,

        /// Set to Some(1) to include the download url in the response.
        #[serde(rename = "g")]
        include_download_url: Option<u8>,
    },

    /// Fetch the nodes
    #[serde(rename = "f")]
    FetchNodes {
        c: u8,

        /// Set to 1 to make this recursive.
        /// Otherwise, leave it as 0.
        #[serde(rename = "r")]
        recursive: u8,
    },
}
