// error.rs — Error types for the MCP gateway.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during MCP gateway operations.
#[derive(Debug, Error)]
pub enum GatewayError {
    /// A goal lifecycle operation failed.
    #[error("goal error: {0}")]
    Goal(#[from] ta_goal::GoalError),

    /// A workspace operation failed.
    #[error("workspace error: {0}")]
    Workspace(#[from] ta_workspace::WorkspaceError),

    /// A connector operation failed.
    #[error("connector error: {0}")]
    Connector(#[from] ta_connector_fs::FsConnectorError),

    /// Policy denied the requested action.
    #[error("policy denied: {0}")]
    PolicyDenied(String),

    /// The requested goal run was not found.
    #[error("goal not found: {0}")]
    GoalNotFound(Uuid),

    /// No active connector for the given goal.
    #[error("no active connector for goal: {0}")]
    NoConnector(Uuid),

    /// An I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A generic error.
    #[error("{0}")]
    Other(String),
}

/// Convert AuditError to GatewayError (can't use From because AuditError
/// contains io::Error which conflicts with our Io variant).
impl From<ta_audit::AuditError> for GatewayError {
    fn from(e: ta_audit::AuditError) -> Self {
        GatewayError::Other(e.to_string())
    }
}
