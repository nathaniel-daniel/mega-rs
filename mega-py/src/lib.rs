use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::sync::LazyLock;

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

        todo!()
    }
}

#[pyclass]
pub struct File {}

/// An API for mega.
#[pymodule]
fn mega_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Client>()?;
    Ok(())
}
