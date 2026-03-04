// error.rs — Error types for the mediation crate.

use std::path::PathBuf;

/// Errors from resource mediation operations.
#[derive(Debug, thiserror::Error)]
pub enum MediationError {
    #[error("unsupported scheme: '{scheme}'")]
    UnsupportedScheme { scheme: String },

    #[error("staging failed for {uri}: {reason}")]
    StagingFailed { uri: String, reason: String },

    #[error("apply failed for {uri}: {reason}")]
    ApplyFailed { uri: String, reason: String },

    #[error("rollback failed for {uri}: {reason}")]
    RollbackFailed { uri: String, reason: String },

    #[error("no mediator registered for scheme '{scheme}'")]
    NoMediator { scheme: String },

    #[error("invalid URI: '{uri}'")]
    InvalidUri { uri: String },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("workspace error: {0}")]
    Workspace(String),
}
