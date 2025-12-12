mod model;

pub use self::model::NodeKind;
use mega::EasyFileDownloadReader;
use mega::FileOrFolderKey;
use mega::FolderKey;
use mega::ParsedMegaUrl;
use mega::Url;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pythonize::depythonize;
use pythonize::pythonize;
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

struct DisplayPythonOptional<T>(Option<T>);

impl<T> std::fmt::Debug for DisplayPythonOptional<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.0 {
            Some(value) => write!(f, "{value:?}"),
            None => write!(f, "None"),
        }
    }
}

impl<T> std::fmt::Display for DisplayPythonOptional<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.0 {
            Some(value) => write!(f, "{value}"),
            None => write!(f, "None"),
        }
    }
}

/// A mega node, a file or folder
#[pyclass(module = "mega_py")]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Node {
    /// The public id of a node.
    #[pyo3(get)]
    pub public_id: Option<String>,

    /// The id of a node.
    #[pyo3(get)]
    pub id: Option<String>,

    #[pyo3(get)]
    pub name: String,

    /// The key for this node.
    key: FileOrFolderKey,

    /// The public id of the parent folder
    #[pyo3(get)]
    pub parent_public_id: Option<String>,

    /// The key of the parent folder
    parent_key: Option<FolderKey>,

    /// The node kind
    #[serde(rename = "type")]
    kind: NodeKind,
}

#[pymethods]
impl Node {
    #[getter]
    pub fn key(&self) -> String {
        self.key.to_string()
    }

    #[getter]
    pub fn get_type(&self) -> String {
        self.kind.to_string()
    }

    #[getter]
    pub fn parent_key(&self) -> Option<String> {
        self.parent_key.map(|key| key.to_string())
    }

    /// Serialize this as a dict.
    pub fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let value = pythonize(py, self)?;
        Ok(value)
    }

    /// Deserialize this from a dict.
    #[staticmethod]
    pub fn from_dict(value: Bound<'_, PyAny>) -> PyResult<Self> {
        let value = depythonize(&value)?;
        Ok(value)
    }

    pub fn __repr__(&self) -> String {
        let public_id = DisplayPythonOptional(self.public_id.as_deref());
        let id = DisplayPythonOptional(self.id.as_deref());
        let name = &self.name;
        let kind = self.kind.as_str();
        let key = &self.key;
        let parent_public_id = DisplayPythonOptional(self.parent_public_id.as_deref());
        let parent_key = DisplayPythonOptional(self.parent_key.as_ref());

        format!("File(public_id={public_id:?}, id={id:?}, name={name:?}, type={kind:?}, parent_public_id={parent_public_id:?}, key={key:?}, parent_key={parent_key:?})")
    }
}

/// An entry in a folder listing
#[pyclass(module = "mega_py")]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct FolderEntry {
    /// The id of the node
    #[pyo3(get)]
    pub id: String,

    /// The id of the parent node
    #[pyo3(get)]
    pub parent_id: String,

    /// The name of the node
    #[pyo3(get)]
    pub name: String,

    /// The type of node
    #[serde(rename = "type")]
    kind: NodeKind,

    /// The node's key
    key: mega::FileOrFolderKey,
}

#[pymethods]
impl FolderEntry {
    #[getter]
    pub fn get_type(&self) -> &'static str {
        self.kind.as_str()
    }

    #[getter]
    pub fn key(&self) -> String {
        self.key.to_string()
    }

    /// Try to turn this into a Node.
    pub fn as_node(&self, parent: &str) -> PyResult<Node> {
        let url = Url::parse(parent).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let url = ParsedMegaUrl::try_from(&url)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let folder_url = url
            .as_folder_url()
            .ok_or_else(|| PyRuntimeError::new_err("parent is not a folder url"))?;

        Ok(Node {
            public_id: None,
            id: Some(self.id.clone()),
            name: self.name.clone(),

            key: self.key.clone(),
            parent_public_id: Some(folder_url.folder_id.clone()),
            parent_key: Some(folder_url.folder_key),

            kind: self.kind,
        })
    }

    /// Serialize this as a dict.
    pub fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let value = pythonize(py, self)?;
        Ok(value)
    }

    /// Deserialize this from a dict.
    #[staticmethod]
    pub fn from_dict(value: Bound<'_, PyAny>) -> PyResult<Self> {
        let value = depythonize(&value)?;
        Ok(value)
    }

    pub fn __repr__(&self) -> String {
        let id = &self.id;
        let parent_id = &self.parent_id;
        let name = &self.name;
        let key = &self.key;
        let kind = &self.kind;

        format!("FolderEntry(id={id:?}, parent_id={parent_id:?}, name={name:?}, key=\"{key}\", type=\"{kind}\")")
    }
}

#[pyclass(module = "mega_py")]
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

    /// Get a node from a url.
    pub fn get_node_from_url(&self, url: &str) -> PyResult<Node> {
        let tokio_rt = get_tokio_rt()?;

        let url = Url::parse(url).map_err(|error| PyValueError::new_err(error.to_string()))?;
        let url = ParsedMegaUrl::try_from(&url)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;

        match url {
            ParsedMegaUrl::File(file_url) => {
                let (_attributes, decoded_attributes) = tokio_rt
                    .block_on(async {
                        let mut builder = mega::EasyGetAttributesBuilder::new();
                        builder.public_node_id(file_url.file_id.clone());

                        let attributes = self.client.get_attributes(builder).await?;
                        let decoded_attributes =
                            attributes.decode_attributes(file_url.file_key.key)?;

                        Result::<_, mega::Error>::Ok((attributes, decoded_attributes))
                    })
                    .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

                Ok(Node {
                    public_id: Some(file_url.file_id),
                    id: None,
                    name: decoded_attributes.name,

                    key: file_url.file_key.into(),
                    parent_public_id: None,
                    parent_key: None,

                    // Assume file node.
                    kind: NodeKind::File,
                })
            }
            ParsedMegaUrl::Folder(folder_url) => match folder_url.child_data {
                Some(child_data) => {
                    let fetch_nodes_response = tokio_rt
                        .block_on(async {
                            self.client
                                .fetch_nodes(Some(&folder_url.folder_id), true)
                                .await
                        })
                        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
                    let node_entry = fetch_nodes_response
                        .nodes
                        .iter()
                        .find(|node| node.id == child_data.node_id)
                        .ok_or_else(|| {
                            PyRuntimeError::new_err("missing file node in folder listing")
                        })?;
                    let node_key = node_entry
                        .decrypt_key(&folder_url.folder_key)
                        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
                    let decoded_attributes =
                        node_entry
                            .decode_attributes(&folder_url.folder_key)
                            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

                    match (child_data.is_file, node_key.is_file_key()) {
                        (true, false) => return Err(PyRuntimeError::new_err("node is not a file")),
                        (false, true) => {
                            return Err(PyRuntimeError::new_err("node is not a folder"))
                        }
                        _ => {}
                    }

                    Ok(Node {
                        public_id: None,
                        id: Some(node_entry.id.clone()),
                        name: decoded_attributes.name,

                        key: node_key,
                        parent_public_id: Some(folder_url.folder_id),
                        parent_key: Some(folder_url.folder_key),

                        kind: node_entry.kind.try_into()?,
                    })
                }
                None => {
                    let fetch_nodes_response = tokio_rt
                        .block_on(async {
                            self.client
                                .fetch_nodes(Some(&folder_url.folder_id), false)
                                .await
                        })
                        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

                    let folder_entry = fetch_nodes_response
                        .nodes
                        .first()
                        .ok_or_else(|| PyRuntimeError::new_err("missing files"))?;

                    let decoded_attributes = folder_entry
                        .decode_attributes(&folder_url.folder_key)
                        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

                    Ok(Node {
                        public_id: Some(folder_url.folder_id.clone()),
                        id: Some(folder_entry.id.clone()),
                        name: decoded_attributes.name,

                        key: folder_url.folder_key.into(),
                        parent_public_id: Some(folder_url.folder_id),
                        parent_key: Some(folder_url.folder_key),

                        kind: folder_entry.kind.try_into()?,
                    })
                }
            },
        }
    }

    /// Get a file by url.
    #[pyo3(signature=(url=None, node_id=None, key=None))]
    pub fn get_file(
        &self,
        url: Option<&str>,
        mut node_id: Option<String>,
        key: Option<&str>,
    ) -> PyResult<Node> {
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
            let parsed_url = ParsedMegaUrl::try_from(&url)
                .map_err(|error| PyValueError::new_err(error.to_string()))?;
            match parsed_url {
                ParsedMegaUrl::File(file_url) => {
                    public_file_id = Some(file_url.file_id.to_string());
                    file_key = Some(file_url.file_key);
                }
                ParsedMegaUrl::Folder(folder_url) => {
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
                if let Some(public_node_id) = public_file_id.as_ref() {
                    builder.public_node_id(public_node_id);
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

        Ok(Node {
            public_id: public_file_id,
            id: node_id.map(|value| value.to_string()),
            name: decoded_attributes.name,

            key: file_key.into(),
            parent_public_id: None,
            parent_key: None,

            // Assume file
            kind: NodeKind::File,
        })
    }

    /// Start a download for a file.
    pub fn download_file(&self, file: &Node) -> PyResult<FileDownload> {
        let tokio_rt = get_tokio_rt()?;

        let file_key = file
            .key
            .as_file_key()
            .ok_or_else(|| PyRuntimeError::new_err("node is a folder"))?;

        let reader = tokio_rt.block_on(async {
            let mut builder = mega::EasyGetAttributesBuilder::new();
            builder.include_download_url(true);
            if let Some(public_id) = file.public_id.as_ref() {
                builder.public_node_id(public_id);
            }
            if let Some(id) = file.id.as_ref() {
                builder.node_id(id);
            }
            if let Some(parent_public_id) = file.parent_public_id.clone() {
                builder.reference_node_id(parent_public_id);
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
                .download_file(file_key, download_url.as_str())
                .await
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

            Result::<_, PyErr>::Ok(reader)
        })?;

        Ok(FileDownload { reader })
    }

    /// List files in a folder.
    ///
    /// This will work off of the parent node of the provided node.
    #[pyo3(signature = (node, recursive = false))]
    pub fn list_files(&self, node: &Node, recursive: bool) -> PyResult<Vec<FolderEntry>> {
        let tokio_rt = get_tokio_rt()?;

        let parent_key = node
            .parent_key
            .ok_or_else(|| PyRuntimeError::new_err("missing parent public key"))?;
        let public_node_id = node
            .parent_public_id
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("missing parent public node id"))?;

        let fetch_nodes_response = tokio_rt
            .block_on(async {
                self.client
                    .fetch_nodes(Some(public_node_id), recursive)
                    .await
            })
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

        let mut items = Vec::new();
        for node in fetch_nodes_response.nodes.into_iter() {
            let key = node
                .decrypt_key(&parent_key)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
            let attributes = node
                .decode_attributes(&parent_key)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

            items.push(FolderEntry {
                id: node.id,
                parent_id: node.parent_id,
                name: attributes.name,
                key,
                kind: node.kind.try_into()?,
            });
        }

        Ok(items)
    }
}

#[pyclass(module = "mega_py")]
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

/// An API for mega.
#[pymodule]
fn mega_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Node>()?;
    m.add_class::<FileDownload>()?;
    m.add_class::<FolderEntry>()?;
    m.add_class::<Client>()?;
    Ok(())
}
