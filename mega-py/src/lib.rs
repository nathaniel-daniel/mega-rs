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
    pub fn get_file(&self, url: &str) -> PyResult<File> {
        let tokio_rt = get_tokio_rt()?;

        let url = Url::parse(url).map_err(|error| PyValueError::new_err(error.to_string()))?;
        let parsed_url =
            mega::parse_file_url(&url).map_err(|error| PyValueError::new_err(error.to_string()))?;

        let decoded_attributes = tokio_rt
            .block_on(async {
                let attributes_future = self.client.get_attributes(parsed_url.file_id, false);
                self.client.send_commands();

                let attributes = attributes_future.await?;
                let decoded_attributes = attributes.decode_attributes(parsed_url.file_key.key)?;

                Result::<_, mega::Error>::Ok(decoded_attributes)
            })
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

        Ok(File {
            id: parsed_url.file_id.to_string(),
            name: decoded_attributes.name,
            key: parsed_url.file_key,
        })
    }

    /// Start a download for a file.
    pub fn download_file(&self, file: &File) -> PyResult<FileDownload> {
        let tokio_rt = get_tokio_rt()?;

        let reader = tokio_rt.block_on(async {
            let attributes_future = self.client.get_attributes(&file.id, true);
            self.client.send_commands();

            let attributes = attributes_future
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
}

#[pyclass]
pub struct File {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub name: String,
    key: FileKey,
}

#[pymethods]
impl File {
    pub fn __repr__(&self) -> String {
        let id = &self.id;
        let name = &self.name;

        format!("File(id={id}, name={name})")
    }
}

#[pyclass]
pub struct FileDownload {
    reader: EasyFileDownloadReader<Pin<Box<dyn AsyncRead + Send + Sync>>>,
}

#[pymethods]
impl FileDownload {
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
    m.add_class::<Client>()?;
    Ok(())
}
