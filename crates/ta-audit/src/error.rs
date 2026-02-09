// error.rs — Error types for the audit subsystem.
//
// Uses `thiserror` to derive the standard Rust `Error` trait automatically.
// Each variant maps to a specific failure mode in the audit pipeline.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during audit operations.
#[derive(Debug, Error)]
pub enum AuditError {
    /// Failed to open or create the audit log file.
    #[error("failed to open audit log at {path}: {source}")]
    OpenFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Failed to write an event to the log.
    #[error("failed to append event: {0}")]
    WriteFailed(#[from] std::io::Error),

    /// Failed to serialize or deserialize an event (malformed JSON).
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// The audit log has been tampered with — hash chain is broken.
    #[error("integrity check failed at line {line}: expected hash {expected}, got {actual}")]
    IntegrityViolation {
        line: usize,
        expected: String,
        actual: String,
    },

    /// Failed to read a file for hashing.
    #[error("failed to hash file at {path}: {source}")]
    HashFileFailed {
        path: PathBuf,
        source: std::io::Error,
    },
}
