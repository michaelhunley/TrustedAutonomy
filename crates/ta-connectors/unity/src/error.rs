use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnityConnectorError {
    #[error("Unity MCP server not reachable at '{0}': {1}")]
    NotReachable(String, String),
    #[error("Unity project not found at path '{0}'")]
    ProjectNotFound(String),
    #[error("build failed: {0}")]
    BuildFailed(String),
    #[error("scene not found: '{0}'")]
    SceneNotFound(String),
    #[error("test run failed: {0}")]
    TestFailed(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(String),
    #[error("protocol error: {0}")]
    Protocol(String),
}
