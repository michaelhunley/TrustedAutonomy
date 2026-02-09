// error.rs — Error types for the filesystem connector.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during filesystem connector operations.
#[derive(Debug, Error)]
pub enum FsConnectorError {
    /// A file I/O operation failed.
    #[error("I/O error at {path}: {source}")]
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The staging workspace returned an error.
    #[error("workspace error: {0}")]
    WorkspaceError(#[from] ta_workspace::WorkspaceError),

    /// The audit log returned an error.
    #[error("audit error: {0}")]
    AuditError(#[from] ta_audit::AuditError),

    /// Attempted to apply changes without approval.
    #[error("cannot apply changes: not approved (current status: {status})")]
    NotApproved { status: String },

    /// No changes have been staged to build a PR package from.
    #[error("no staged changes for goal '{goal_id}'")]
    NoStagedChanges { goal_id: String },

    /// A path traversal attempt was detected.
    #[error("path traversal detected: '{path}'")]
    PathTraversal { path: String },
}
