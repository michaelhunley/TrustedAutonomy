use serde::{Deserialize, Serialize};

use crate::error::UnityConnectorError;

/// Result of a Unity Player or AssetBundle build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    /// Whether the build succeeded.
    pub success: bool,
    /// Build output directory path (relative to project root).
    pub output_path: String,
    /// Human-readable build log summary.
    pub log_summary: String,
}

/// A single GameObject in a Unity scene hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameObjectInfo {
    /// Instance ID of the GameObject.
    pub instance_id: i64,
    /// Name of the GameObject.
    pub name: String,
    /// Tag (e.g. "MainCamera", "Player", "Untagged").
    pub tag: String,
    /// Layer index.
    pub layer: i32,
    /// Whether the GameObject is active in the hierarchy.
    pub active: bool,
    /// Component type names attached to this GameObject.
    pub components: Vec<String>,
    /// Instance IDs of direct children.
    pub children: Vec<i64>,
}

/// Scene hierarchy returned by `unity_scene_query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneInfo {
    /// Scene asset path (e.g. "Assets/Scenes/Main.unity").
    pub scene_path: String,
    /// Root-level GameObjects in the scene.
    pub root_objects: Vec<GameObjectInfo>,
    /// Total count of all GameObjects in the scene.
    pub total_objects: usize,
}

/// Summary of a Unity test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRunResult {
    /// Number of tests that passed.
    pub passed: usize,
    /// Number of tests that failed.
    pub failed: usize,
    /// Number of tests that were skipped/ignored.
    pub skipped: usize,
    /// Short descriptions of failed tests (up to 10).
    pub failures: Vec<String>,
    /// Total test run duration in seconds.
    pub duration_secs: f64,
}

/// Result of a render capture operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCaptureResult {
    /// Path to the captured image file (relative to project root).
    pub output_path: String,
    /// Width of the captured image in pixels.
    pub width: u32,
    /// Height of the captured image in pixels.
    pub height: u32,
}

/// Trait implemented by each Unity backend ("official", "community", "stub").
pub trait UnityBackend: Send + Sync {
    /// Human-readable backend name.
    fn name(&self) -> &str;

    /// TCP socket address this backend targets.
    fn socket_addr(&self) -> &str;

    /// Trigger a Unity Player or AssetBundle build.
    ///
    /// `target`: build target string (e.g. "StandaloneOSX", "StandaloneWindows64", "WebGL",
    ///           "AssetBundle").
    /// `config`: optional build configuration ("Debug" or "Release").
    fn build_trigger(
        &self,
        target: &str,
        config: Option<&str>,
    ) -> Result<BuildResult, UnityConnectorError>;

    /// Query the GameObject hierarchy of a Unity scene.
    ///
    /// `scene_path`: asset path of the scene (e.g. "Assets/Scenes/Main.unity").
    ///              Pass an empty string to query the currently-open scene.
    fn scene_query(&self, scene_path: &str) -> Result<SceneInfo, UnityConnectorError>;

    /// Run Unity EditMode or PlayMode tests matching an optional filter.
    ///
    /// `filter`: optional test name filter (substring match). Pass None to run all tests.
    fn test_run(&self, filter: Option<&str>) -> Result<TestRunResult, UnityConnectorError>;

    /// Trigger an Addressables content build.
    fn addressables_build(&self) -> Result<BuildResult, UnityConnectorError>;

    /// Capture a screenshot from a named scene camera.
    ///
    /// `camera_path`: GameObject path to the camera (e.g. "/Main Camera").
    /// `output_path`: destination file path for the PNG (relative to project root).
    fn render_capture(
        &self,
        camera_path: &str,
        output_path: &str,
    ) -> Result<RenderCaptureResult, UnityConnectorError>;
}
