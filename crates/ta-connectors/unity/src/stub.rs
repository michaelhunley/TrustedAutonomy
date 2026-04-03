// stub.rs — In-memory stub backend for tests and offline development.

use crate::{
    backend::{
        BuildResult, GameObjectInfo, RenderCaptureResult, SceneInfo, TestRunResult, UnityBackend,
    },
    error::UnityConnectorError,
};

/// Stub backend that returns deterministic responses without contacting Unity.
pub struct StubBackend {
    socket: String,
}

impl StubBackend {
    pub fn new(socket: &str) -> Self {
        Self {
            socket: socket.to_string(),
        }
    }
}

impl UnityBackend for StubBackend {
    fn name(&self) -> &str {
        "stub"
    }

    fn socket_addr(&self) -> &str {
        &self.socket
    }

    fn build_trigger(
        &self,
        target: &str,
        config: Option<&str>,
    ) -> Result<BuildResult, UnityConnectorError> {
        let cfg = config.unwrap_or("Release");
        Ok(BuildResult {
            success: true,
            output_path: format!("Builds/{}/{}", cfg, target),
            log_summary: format!(
                "Stub build succeeded for target '{}' (config: {}).",
                target, cfg
            ),
        })
    }

    fn scene_query(&self, scene_path: &str) -> Result<SceneInfo, UnityConnectorError> {
        let path = if scene_path.is_empty() {
            "Assets/Scenes/SampleScene.unity".to_string()
        } else {
            scene_path.to_string()
        };

        let camera = GameObjectInfo {
            instance_id: 1,
            name: "Main Camera".to_string(),
            tag: "MainCamera".to_string(),
            layer: 0,
            active: true,
            components: vec!["Transform".into(), "Camera".into(), "AudioListener".into()],
            children: vec![],
        };

        let light = GameObjectInfo {
            instance_id: 2,
            name: "Directional Light".to_string(),
            tag: "Untagged".to_string(),
            layer: 0,
            active: true,
            components: vec!["Transform".into(), "Light".into()],
            children: vec![],
        };

        Ok(SceneInfo {
            scene_path: path,
            root_objects: vec![camera, light],
            total_objects: 2,
        })
    }

    fn test_run(&self, filter: Option<&str>) -> Result<TestRunResult, UnityConnectorError> {
        let _ = filter;
        Ok(TestRunResult {
            passed: 12,
            failed: 0,
            skipped: 1,
            failures: vec![],
            duration_secs: 1.42,
        })
    }

    fn addressables_build(&self) -> Result<BuildResult, UnityConnectorError> {
        Ok(BuildResult {
            success: true,
            output_path: "ServerData/StandaloneOSX".to_string(),
            log_summary: "Stub Addressables build succeeded. 3 bundles written.".to_string(),
        })
    }

    fn render_capture(
        &self,
        camera_path: &str,
        output_path: &str,
    ) -> Result<RenderCaptureResult, UnityConnectorError> {
        let _ = camera_path;
        Ok(RenderCaptureResult {
            output_path: output_path.to_string(),
            width: 1920,
            height: 1080,
        })
    }
}
