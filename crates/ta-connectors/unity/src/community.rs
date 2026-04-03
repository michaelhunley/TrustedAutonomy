// community.rs — Stub backend for third-party Unity MCP servers.
//
// Community servers (e.g. CoderGamester/unity-mcp, justinpbarnett/unity-mcp)
// use varied protocols. This backend acts as a placeholder that surfaces a
// clear "not configured" message rather than a cryptic error, so agents can
// advise the user to switch to the official backend or configure the correct
// socket/protocol manually.

use crate::{
    backend::{BuildResult, RenderCaptureResult, SceneInfo, TestRunResult, UnityBackend},
    error::UnityConnectorError,
};

pub struct CommunityBackend {
    socket: String,
}

impl CommunityBackend {
    pub fn new(socket: &str) -> Self {
        Self {
            socket: socket.to_string(),
        }
    }
}

impl UnityBackend for CommunityBackend {
    fn name(&self) -> &str {
        "community"
    }

    fn socket_addr(&self) -> &str {
        &self.socket
    }

    fn build_trigger(
        &self,
        _target: &str,
        _config: Option<&str>,
    ) -> Result<BuildResult, UnityConnectorError> {
        Err(UnityConnectorError::NotReachable(
            self.socket.clone(),
            "community backend is not yet implemented — use backend = \"official\"".to_string(),
        ))
    }

    fn scene_query(&self, _scene_path: &str) -> Result<SceneInfo, UnityConnectorError> {
        Err(UnityConnectorError::NotReachable(
            self.socket.clone(),
            "community backend is not yet implemented — use backend = \"official\"".to_string(),
        ))
    }

    fn test_run(&self, _filter: Option<&str>) -> Result<TestRunResult, UnityConnectorError> {
        Err(UnityConnectorError::NotReachable(
            self.socket.clone(),
            "community backend is not yet implemented — use backend = \"official\"".to_string(),
        ))
    }

    fn addressables_build(&self) -> Result<BuildResult, UnityConnectorError> {
        Err(UnityConnectorError::NotReachable(
            self.socket.clone(),
            "community backend is not yet implemented — use backend = \"official\"".to_string(),
        ))
    }

    fn render_capture(
        &self,
        _camera_path: &str,
        _output_path: &str,
    ) -> Result<RenderCaptureResult, UnityConnectorError> {
        Err(UnityConnectorError::NotReachable(
            self.socket.clone(),
            "community backend is not yet implemented — use backend = \"official\"".to_string(),
        ))
    }
}
