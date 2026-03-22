use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Proxy start failed: {0}")]
    StartFailed(String),
    #[error("Query parsing error: {0}")]
    ParseError(String),
    #[error("Overlay error: {0}")]
    Overlay(#[from] ta_db_overlay::OverlayError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Plugin error: {0}")]
    Plugin(String),
}

pub type Result<T> = std::result::Result<T, ProxyError>;
