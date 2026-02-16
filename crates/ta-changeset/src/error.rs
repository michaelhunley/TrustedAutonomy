// error.rs — Error types for the changeset subsystem.

use thiserror::Error;

/// Errors that can occur during changeset operations.
#[derive(Debug, Error)]
pub enum ChangeSetError {
    /// Invalid status transition (e.g., Committed → Draft).
    #[error("invalid status transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    /// Serialization or deserialization failure.
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Invalid or malformed data.
    #[error("invalid data: {0}")]
    InvalidData(String),
}
