use url::Url;

/// An api response
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum Response<T> {
    /// Error
    ///
    /// There was en error with the specified code.
    Error(ErrorCode),

    /// Success
    ///
    /// There is valid data
    Ok(T),
}

impl<T> Response<T> {
    /// Unwrap the response, panicing on failure.
    ///
    /// Intended for quick testing and scripting.
    pub fn unwrap(self) -> T {
        match self {
            Self::Error(error) => panic!("Called 'unwrap' on Error({error:#?})"),
            Self::Ok(t) => t,
        }
    }
}

/// An API Error
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ErrorCode(i32);

/// API Response data
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum ResponseData {
    /// Response for a GetAttributes command
    GetAttributes(GetAttributes),
}

/// GetAttributes command response
#[derive(Debug, serde::Serialize, serde:: Deserialize)]
pub struct GetAttributes {
    /// The file size
    #[serde(rename = "s")]
    pub size: u64,

    pub at: String,
    pub msd: u8,

    /// The download url
    #[serde(rename = "g")]
    pub download_url: Option<Url>,
}
