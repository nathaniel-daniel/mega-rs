use crate::FileKey;
use crate::FileKeyParseError;
use crate::FolderKey;
use crate::FolderKeyParseError;
use url::Url;

/// An error that may occur while parsing a mega url.
#[derive(Debug, thiserror::Error)]
pub enum ParseMegaUrlError {
    #[error("invalid file key")]
    InvalidFileKey(#[source] FileKeyParseError),

    #[error("invalid folder key")]
    InvalidFolderKey(#[source] FolderKeyParseError),

    #[error("{0}")]
    Generic(&'static str),
}

/// A parsed mega url.
#[derive(Debug)]
pub enum ParsedMegaUrl {
    File(ParsedMegaFileUrl),
    Folder(ParsedMegaFolderUrl),
}

impl ParsedMegaUrl {
    /// Get as ref to the file url struct if it is a file url.
    pub fn as_file_url(&self) -> Option<&ParsedMegaFileUrl> {
        match self {
            Self::File(file) => Some(file),
            _ => None,
        }
    }

    /// Get as ref to the folder url struct if it is a folder url.
    pub fn as_folder_url(&self) -> Option<&ParsedMegaFolderUrl> {
        match self {
            Self::Folder(file) => Some(file),
            _ => None,
        }
    }
}

impl TryFrom<&Url> for ParsedMegaUrl {
    type Error = ParseMegaUrlError;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        if url.host_str() != Some("mega.nz") {
            return Err(ParseMegaUrlError::Generic("invalid host"));
        }

        let mut path_iter = url
            .path_segments()
            .ok_or(ParseMegaUrlError::Generic("missing path"))?;

        match path_iter.next() {
            Some("file") => {
                let file_id = path_iter
                    .next()
                    .ok_or(ParseMegaUrlError::Generic("missing file id path segment"))?;

                if path_iter.next().is_some() {
                    return Err(ParseMegaUrlError::Generic(
                        "expected the path to end, but it continued",
                    ));
                }

                let file_key_raw = url
                    .fragment()
                    .ok_or(ParseMegaUrlError::Generic("missing file key"))?;
                let file_key: FileKey = file_key_raw
                    .parse()
                    .map_err(ParseMegaUrlError::InvalidFileKey)?;

                Ok(Self::File(ParsedMegaFileUrl {
                    file_id: file_id.to_string(),
                    file_key,
                }))
            }
            Some("folder") => {
                let folder_id = path_iter
                    .next()
                    .ok_or(ParseMegaUrlError::Generic("missing folder id path segment"))?;

                if path_iter.next().is_some() {
                    return Err(ParseMegaUrlError::Generic(
                        "expected the path to end, but it continued",
                    ));
                }

                let folder_key_raw = url
                    .fragment()
                    .ok_or(ParseMegaUrlError::Generic("missing folder key"))?;
                let (folder_key_raw, rest) = folder_key_raw
                    .split_once('/')
                    .unwrap_or((folder_key_raw, ""));

                let child_data = if !rest.is_empty() {
                    let (kind, node_id) = rest
                        .split_once('/')
                        .ok_or(ParseMegaUrlError::Generic("unknown fragment format"))?;

                    let is_file = match kind {
                        "file" => true,
                        "folder" => false,
                        _ => {
                            return Err(ParseMegaUrlError::Generic(
                                "unknown fragment path segment",
                            ));
                        }
                    };

                    Some(ParsedMegaFolderUrlChildData {
                        is_file,
                        node_id: node_id.to_string(),
                    })
                } else {
                    None
                };

                let folder_key: FolderKey = folder_key_raw
                    .parse()
                    .map_err(ParseMegaUrlError::InvalidFolderKey)?;

                Ok(Self::Folder(ParsedMegaFolderUrl {
                    folder_id: folder_id.to_string(),
                    folder_key,
                    child_data,
                }))
            }
            Some(_) => Err(ParseMegaUrlError::Generic("unknown path segment")),
            None => Err(ParseMegaUrlError::Generic("missing path segment")),
        }
    }
}

/// A parsed file url
#[derive(Debug)]
pub struct ParsedMegaFileUrl {
    /// The public file id
    pub file_id: String,

    /// The file key
    pub file_key: FileKey,
}

/// A parsed folder url
#[derive(Debug)]
pub struct ParsedMegaFolderUrl {
    /// The folder id
    pub folder_id: String,

    ///The folder key
    pub folder_key: FolderKey,

    /// Child data
    pub child_data: Option<ParsedMegaFolderUrlChildData>,
}

#[derive(Debug)]
pub struct ParsedMegaFolderUrlChildData {
    /// If true, this is a file. Otherwise, it is a folder.
    pub is_file: bool,

    /// The node id.
    pub node_id: String,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::*;

    #[test]
    fn test_parse_file_url() {
        let url = Url::parse(TEST_FILE).unwrap();

        let parsed = ParsedMegaUrl::try_from(&url).expect("failed to parse url");
        let parsed = parsed.as_file_url().expect("not a file url");
        assert!(parsed.file_id == TEST_FILE_ID);
        assert!(parsed.file_key.key == TEST_FILE_KEY_KEY_DECODED);
        assert!(parsed.file_key.iv == TEST_FILE_KEY_IV_DECODED);
        assert!(parsed.file_key.meta_mac == TEST_FILE_META_MAC_DECODED);
    }

    #[test]
    fn test_parse_folder_url() {
        let url = Url::parse(TEST_FOLDER).unwrap();

        let parsed = ParsedMegaUrl::try_from(&url).expect("failed to parse url");
        let parsed = parsed.as_folder_url().expect("not a folder url");
        assert!(parsed.folder_id == "MWsm3aBL");
        assert!(parsed.folder_key.0 == TEST_FOLDER_KEY_DECODED);
        assert!(parsed.child_data.is_none());
    }

    #[test]
    fn test_parse_folder_nested_url() {
        let url = Url::parse(TEST_FOLDER_NESTED).unwrap();

        let parsed = ParsedMegaUrl::try_from(&url).expect("failed to parse url");
        let parsed = parsed.as_folder_url().expect("not a folder url");
        assert!(parsed.folder_id == "MWsm3aBL");
        assert!(parsed.folder_key.0 == TEST_FOLDER_KEY_DECODED);
        let child_data = parsed.child_data.as_ref().expect("missing child data");
        assert!(!child_data.is_file);
        assert!(child_data.node_id == "IGlBlD6K");
    }
}
