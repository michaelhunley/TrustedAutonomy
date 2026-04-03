use serde::{Deserialize, Serialize};

/// Top-level `[connectors.unity]` config block.
///
/// Example:
/// ```toml
/// [connectors.unity]
/// enabled = true
/// backend = "official"
/// project_path = "/path/to/MyProject"
/// socket = "localhost:30200"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnityConnectorConfig {
    /// Whether the Unity connector is active.
    #[serde(default)]
    pub enabled: bool,

    /// Backend to use: "official" (com.unity.mcp-server) or "community".
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Filesystem path to the Unity project root (the folder containing Assets/).
    /// Empty string means not configured.
    #[serde(default)]
    pub project_path: String,

    /// TCP address where the Unity MCP server is listening.
    #[serde(default = "default_socket")]
    pub socket: String,
}

fn default_backend() -> String {
    "official".to_string()
}

fn default_socket() -> String {
    "localhost:30200".to_string()
}

impl Default for UnityConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: default_backend(),
            project_path: String::new(),
            socket: default_socket(),
        }
    }
}

impl UnityConnectorConfig {
    pub fn from_toml(s: &str) -> Result<Self, crate::error::UnityConnectorError> {
        toml::from_str(s).map_err(|e| crate::error::UnityConnectorError::Config(e.to_string()))
    }
}
