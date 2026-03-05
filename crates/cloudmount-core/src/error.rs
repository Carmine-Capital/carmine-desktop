use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("Graph API error: {status} {message}")]
    GraphApi { status: u16, message: String },

    #[error("cache error: {0}")]
    Cache(String),

    #[error("filesystem error: {0}")]
    Filesystem(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
