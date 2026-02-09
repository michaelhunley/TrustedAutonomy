// error.rs — Error types for the policy subsystem.

use thiserror::Error;

/// Errors that can occur during policy operations.
#[derive(Debug, Error)]
pub enum PolicyError {
    /// The requested agent has no capability manifest loaded.
    #[error("no manifest found for agent '{agent_id}'")]
    NoManifest { agent_id: String },

    /// The capability manifest has expired.
    #[error("manifest for agent '{agent_id}' expired at {expired_at}")]
    ManifestExpired {
        agent_id: String,
        expired_at: String,
    },

    /// A resource pattern is malformed and cannot be parsed as a glob.
    #[error("invalid resource pattern '{pattern}': {reason}")]
    InvalidPattern { pattern: String, reason: String },

    /// The target URI contains path traversal sequences (security violation).
    #[error("path traversal detected in target URI: '{uri}'")]
    PathTraversal { uri: String },
}
