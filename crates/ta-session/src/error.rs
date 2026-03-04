// error.rs — Error types for the session crate.

/// Errors from session operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session not found: {session_id}")]
    NotFound { session_id: String },

    #[error("invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    #[error("session already exists for goal {goal_id}")]
    AlreadyExists { goal_id: String },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
