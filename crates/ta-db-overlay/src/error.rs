use thiserror::Error;

#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Invalid resource URI: {0}")]
    InvalidUri(String),
}

pub type Result<T> = std::result::Result<T, OverlayError>;
