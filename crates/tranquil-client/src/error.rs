use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API {0}: {1}")]
    Api(u16, serde_json::Value),

    #[error("Auth: {0}")]
    Auth(String),

    #[error("Config: {0}")]
    Config(String),

    #[error("{0}")]
    Other(String),
}