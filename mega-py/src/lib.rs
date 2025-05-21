use mega::EasyFileDownloadReader;
use mega::FileKey;
use mega::Url;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::pin::Pin;
use std::sync::LazyLock;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;

static TOKIO_RT: LazyLock<std::io::Result<tokio::runtime::Runtime>> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
});

fn get_tokio_rt() -> PyResult<&'static tokio::runtime::Runtime> {
    TOKIO_RT
        .as_ref()
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))
}

#[pyclass]
pub struct Client {
    client: mega::EasyClient,
}

#[pymethods]
impl Client {
    #[new]
    fn new() -> Self {
        Client {
            client: mega::EasyClient::new(),
        }
    }

    /// Get a file by url.
    #[pyo3(signature=(url=None, node_id=None, key=None))]
    pub fn get_file(
        &self,
        url: Option<&str>,
        mut node_id: Option<String>,
        key: Option<&str>,
    ) -> PyResult<File> {
        let tokio_rt = get_tokio_rt()?;

        if url.is_some() && node_id.is_some() {
            return Err(PyValueError::new_err(
                "url and node_id are mutually exclusive",
            ));
        }

        let mut public_file_id = None;
        let mut file_key = None;
        let mut reference_node_id = None;
        if let Some(url) = url {
            let url = Url::parse(url).map_err(|error| PyValueError::new_err(error.to_string()))?;
            let parsed_url = mega::ParsedMegaUrl::try_from(&url)
                .map_err(|error| PyValueError::new_err(error.to_string()))?;
            match parsed_url {
                mega::ParsedMegaUrl::File(file_url) => {
                    public_file_id = Some(file_url.file_id.to_string());
                    file_key = Some(file_url.file_key);
                }
                mega::ParsedMegaUrl::Folder(folder_url) => {
                    let child_data = folder_url.child_data.ok_or_else(|| {
                        PyValueError::new_err(
                            "folder urls without child data are currently unsupported",
                        )
                    })?;
                    if !child_data.is_file {
                        return Err(PyValueError::new_err(
                            "folder urls with folder child data are currently unsupported",
                        ));
                    }

                    if node_id.is_none() {
                        node_id = Some(child_data.node_id.clone());
                    }

                    reference_node_id = Some(folder_url.folder_id);
                }
            }
        }

        if let Some(key) = key {
            let parsed = key
                .parse::<mega::FileKey>()
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            file_key = Some(parsed);
        }

        let file_key = file_key.ok_or_else(|| PyValueError::new_err("Missing key"))?;

        let (_attributes, decoded_attributes) = tokio_rt
            .block_on(async {
                let mut builder = mega::EasyGetAttributesBuilder::new();
                if let Some(public_file_id) = public_file_id.as_ref() {
                    builder.public_file_id(public_file_id);
                }
                if let Some(node_id) = node_id.as_ref() {
                    builder.node_id(node_id);
                }
                if let Some(reference_node_id) = reference_node_id.clone() {
                    builder.reference_node_id(reference_node_id);
                }
                let attributes = self.client.get_attributes(builder).await?;
                let decoded_attributes = attributes.decode_attributes(file_key.key)?;

                Result::<_, mega::Error>::Ok((attributes, decoded_attributes))
            })
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

        Ok(File {
            public_id: public_file_id,
            node_id: node_id.map(|value| value.to_string()),
            name: decoded_attributes.name,
            key: file_key,
            reference_node_id,
        })
    }

    /// Start a download for a file.
    pub fn download_file(&self, file: &File) -> PyResult<FileDownload> {
        let tokio_rt = get_tokio_rt()?;

        let reader = tokio_rt.block_on(async {
            let mut builder = mega::EasyGetAttributesBuilder::new();
            builder.include_download_url(true);
            if let Some(public_id) = file.public_id.as_ref() {
                builder.public_file_id(public_id);
            }
            if let Some(node_id) = file.node_id.as_ref() {
                builder.node_id(node_id);
            }
            if let Some(reference_node_id) = file.reference_node_id.clone() {
                builder.reference_node_id(reference_node_id);
            }

            let attributes = self
                .client
                .get_attributes(builder)
                .await
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
            let download_url = attributes
                .download_url
                .ok_or_else(|| PyRuntimeError::new_err("missing download url"))?;

            let reader = self
                .client
                .download_file(&file.key, download_url.as_str())
                .await
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

            Result::<_, PyErr>::Ok(reader)
        })?;

        Ok(FileDownload { reader })
    }

    /// List a folder.
    #[pyo3(signature = (url, recursive = false))]
    pub fn list_folder(&self, url: &str, recursive: bool) -> PyResult<Vec<FolderEntry>> {
        let url = Url::parse(url).map_err(|error| PyValueError::new_err(error.to_string()))?;
        let tokio_rt = get_tokio_rt()?;

        let parsed_url = mega::ParsedMegaUrl::try_from(&url)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let parsed_url = parsed_url
            .as_folder_url()
            .ok_or_else(|| PyValueError::new_err("url must be a folder url"))?;

        if parsed_url.child_data.is_some() {
            return Err(PyValueError::new_err(
                "folder urls with child data are currently unsupported",
            ));
        }

        let response = tokio_rt
            .block_on(async {
                self.client
                    .fetch_nodes(Some(&parsed_url.folder_id), recursive)
                    .await
            })
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

        let mut items = Vec::new();
        for item in response.files.into_iter() {
            let key = item
                .decrypt_key(&parsed_url.folder_key)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
            let attributes = item
                .decode_attributes(&parsed_url.folder_key)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

            items.push(FolderEntry {
                id: item.id,
                name: attributes.name,
                key,
                kind: match item.kind {
                    mega::FetchNodesNodeKind::File => "file".into(),
                    mega::FetchNodesNodeKind::Directory => "folder".into(),
                    _ => "unknown".to_string(),
                },
            });
        }

        Ok(items)
    }
}

#[pyclass]
pub struct File {
    #[pyo3(get)]
    pub public_id: Option<String>,
    #[pyo3(get)]
    pub node_id: Option<String>,
    #[pyo3(get)]
    pub name: String,

    reference_node_id: Option<String>,
    key: FileKey,
}

#[pymethods]
impl File {
    pub fn __repr__(&self) -> String {
        let public_id = &self.public_id;
        let node_id = &self.node_id;
        let name = &self.name;
        let reference_node_id = &self.reference_node_id;
        let key = &self.key;

        format!("File(public_id={public_id:?}, node_id={node_id:?}, name={name:?}, reference_node_id={reference_node_id:?}, key=\"{key}\")")
    }

    #[getter]
    pub fn key(&self) -> String {
        self.key.to_string()
    }
}

#[pyclass]
pub struct FileDownload {
    reader: EasyFileDownloadReader<Pin<Box<dyn AsyncRead + Send + Sync>>>,
}

#[pymethods]
impl FileDownload {
    #[pyo3(signature = (size=Some(-1), /), text_signature = "(size=-1, /)")]
    fn read<'p>(&mut self, size: Option<isize>, py: Python<'p>) -> PyResult<Bound<'p, PyBytes>> {
        let size = match size {
            Some(size) if size > 0 => Some(
                usize::try_from(size).map_err(|error| PyValueError::new_err(error.to_string()))?,
            ),
            Some(0) => return Ok(PyBytes::new(py, &[])),
            Some(_) | None => None,
        };

        let tokio_rt = get_tokio_rt()?;

        let mut buffer = Vec::new();
        tokio_rt.block_on(async {
            match size {
                Some(size) => {
                    buffer.reserve(size);
                    (&mut self.reader)
                        .take(u64::try_from(size).expect("usize is larger than a u64"))
                        .read_to_end(&mut buffer)
                        .await?;
                }
                None => {
                    self.reader.read_to_end(&mut buffer).await?;
                }
            }

            Result::<_, std::io::Error>::Ok(())
        })?;

        Ok(PyBytes::new(py, &buffer))
    }
}

/// A folder listing item
#[pyclass]
pub struct FolderEntry {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get, name = "type")]
    pub kind: String,

    key: mega::FileOrFolderKey,
}

#[pymethods]
impl FolderEntry {
    #[getter]
    pub fn key(&self) -> String {
        self.key.to_string()
    }

    pub fn __repr__(&self) -> String {
        let id = &self.id;
        let name = &self.name;
        let key = &self.key;
        let kind = &self.kind;

        format!("FolderEntry(id={id:?}, name={name:?}, key=\"{key}\", type=\"{kind}\")")
    }
}

/// An API for mega.
#[pymodule]
fn mega_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<File>()?;
    m.add_class::<FileDownload>()?;
    m.add_class::<FolderEntry>()?;
    m.add_class::<Client>()?;
    Ok(())
}
