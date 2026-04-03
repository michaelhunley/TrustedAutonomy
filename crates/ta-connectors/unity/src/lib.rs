pub mod backend;
pub mod community;
pub mod config;
pub mod error;
pub mod official;
pub mod stub;

pub use backend::{
    BuildResult, GameObjectInfo, RenderCaptureResult, SceneInfo, TestRunResult, UnityBackend,
};
pub use community::CommunityBackend;
pub use config::UnityConnectorConfig;
pub use error::UnityConnectorError;
pub use stub::StubBackend;

/// Instantiate the backend specified in config.
///
/// Returns the `official` backend by default. Unsupported backend names fall
/// back to `official` with a warning so callers always get a usable value.
pub fn make_backend(config: &UnityConnectorConfig) -> Box<dyn UnityBackend> {
    match config.backend.as_str() {
        "community" => Box::new(CommunityBackend::new(&config.socket)),
        "stub" => Box::new(StubBackend::new(&config.socket)),
        _ => Box::new(official::OfficialBackend::new(config)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Config tests ─────────────────────────────────────────────────────────

    #[test]
    fn config_defaults_are_correct() {
        let cfg = UnityConnectorConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.backend, "official");
        assert!(cfg.project_path.is_empty());
        assert_eq!(cfg.socket, "localhost:30200");
    }

    #[test]
    fn config_from_toml_full() {
        let toml = r#"
enabled = true
backend = "official"
project_path = "/Users/dev/MyGame"
socket = "localhost:30200"
"#;
        let cfg = UnityConnectorConfig::from_toml(toml).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.backend, "official");
        assert_eq!(cfg.project_path, "/Users/dev/MyGame");
        assert_eq!(cfg.socket, "localhost:30200");
    }

    #[test]
    fn config_from_toml_minimal() {
        let toml = "enabled = true\n";
        let cfg = UnityConnectorConfig::from_toml(toml).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.backend, "official");
        assert_eq!(cfg.socket, "localhost:30200");
    }

    #[test]
    fn config_from_toml_community_backend() {
        let toml = r#"
enabled = true
backend = "community"
socket = "localhost:30201"
"#;
        let cfg = UnityConnectorConfig::from_toml(toml).unwrap();
        assert_eq!(cfg.backend, "community");
        assert_eq!(cfg.socket, "localhost:30201");
    }

    // ── make_backend factory ─────────────────────────────────────────────────

    #[test]
    fn make_backend_official_by_default() {
        let cfg = UnityConnectorConfig::default();
        let backend = make_backend(&cfg);
        assert_eq!(backend.name(), "official");
        assert_eq!(backend.socket_addr(), "localhost:30200");
    }

    #[test]
    fn make_backend_community() {
        let cfg = UnityConnectorConfig {
            backend: "community".to_string(),
            socket: "localhost:30201".to_string(),
            ..Default::default()
        };
        let backend = make_backend(&cfg);
        assert_eq!(backend.name(), "community");
    }

    #[test]
    fn make_backend_stub() {
        let cfg = UnityConnectorConfig {
            backend: "stub".to_string(),
            ..Default::default()
        };
        let backend = make_backend(&cfg);
        assert_eq!(backend.name(), "stub");
    }

    // ── Stub backend behaviour ───────────────────────────────────────────────

    #[test]
    fn stub_build_trigger_succeeds() {
        let stub = StubBackend::new("localhost:30200");
        let result = stub
            .build_trigger("StandaloneOSX", Some("Release"))
            .unwrap();
        assert!(result.success);
        assert!(result.output_path.contains("StandaloneOSX"));
    }

    #[test]
    fn stub_build_trigger_default_config() {
        let stub = StubBackend::new("localhost:30200");
        let result = stub.build_trigger("WebGL", None).unwrap();
        assert!(result.success);
        assert!(result.output_path.contains("WebGL"));
        assert!(result.log_summary.contains("Release"));
    }

    #[test]
    fn stub_scene_query_returns_default_scene() {
        let stub = StubBackend::new("localhost:30200");
        let scene = stub.scene_query("").unwrap();
        assert!(scene.scene_path.ends_with(".unity"));
        assert!(!scene.root_objects.is_empty());
        assert!(scene.root_objects.iter().any(|obj| obj.tag == "MainCamera"));
    }

    #[test]
    fn stub_scene_query_with_path() {
        let stub = StubBackend::new("localhost:30200");
        let scene = stub.scene_query("Assets/Scenes/Level1.unity").unwrap();
        assert_eq!(scene.scene_path, "Assets/Scenes/Level1.unity");
    }

    #[test]
    fn stub_test_run_all_pass() {
        let stub = StubBackend::new("localhost:30200");
        let result = stub.test_run(None).unwrap();
        assert_eq!(result.failed, 0);
        assert!(result.passed > 0);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn stub_addressables_build_succeeds() {
        let stub = StubBackend::new("localhost:30200");
        let result = stub.addressables_build().unwrap();
        assert!(result.success);
        assert!(!result.output_path.is_empty());
    }

    #[test]
    fn stub_render_capture_returns_path() {
        let stub = StubBackend::new("localhost:30200");
        let result = stub
            .render_capture("/Main Camera", "Captures/frame.png")
            .unwrap();
        assert_eq!(result.output_path, "Captures/frame.png");
        assert!(result.width > 0 && result.height > 0);
    }

    #[test]
    fn stub_name_is_stub() {
        let stub = StubBackend::new("localhost:30200");
        assert_eq!(stub.name(), "stub");
    }

    // ── Community backend ────────────────────────────────────────────────────

    #[test]
    fn community_backend_returns_not_reachable() {
        let community = CommunityBackend::new("localhost:30201");
        let err = community.build_trigger("StandaloneOSX", None).unwrap_err();
        assert!(
            err.to_string().contains("community backend"),
            "error should mention community backend: {}",
            err
        );
    }
}
