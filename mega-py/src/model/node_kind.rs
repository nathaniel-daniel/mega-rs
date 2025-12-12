use anyhow::bail;
use std::fmt::Display;
use std::str::FromStr;

/// The node type
#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    /// This node is a file
    #[serde(rename = "file")]
    File,

    /// This node is a folder
    #[serde(rename = "folder")]
    Folder,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Folder => "folder",
        }
    }
}

impl FromStr for NodeKind {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "file" => Ok(Self::File),
            "folder" => Ok(Self::Folder),
            input => bail!("Unknown NodeKind \"{input}\""),
        }
    }
}

impl Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<mega::FetchNodesNodeKind> for NodeKind {
    type Error = anyhow::Error;

    fn try_from(value: mega::FetchNodesNodeKind) -> Result<Self, Self::Error> {
        match value {
            mega::FetchNodesNodeKind::File => Ok(NodeKind::File),
            mega::FetchNodesNodeKind::Directory => Ok(NodeKind::Folder),
            kind => {
                bail!("Unknown NodeKind \"{kind:?}\"");
            }
        }
    }
}
