// tools/unity.rs — Unity Engine tool handlers.
//
// These tools route to a Unity MCP server through the policy engine.
// Build triggers are gated behind `unity://build/**`.
// Test runs are gated behind `unity://test/**`.
// Scene queries are gated behind `unity://scene/**`.
// Render captures are gated behind `unity://render/**`.

use std::sync::{Arc, Mutex};

use rmcp::model::*;
use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use ta_policy::{PolicyEngine, PolicyRequest};

use crate::server::GatewayState;
use crate::validation::enforce_policy;

// ── Parameter types ───────────────────────────────────────────────────────────

/// Parameters for `unity_build_trigger`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnityBuildTriggerParams {
    /// Build target: "StandaloneOSX", "StandaloneWindows64", "WebGL", "AssetBundle", etc.
    pub target: String,
    /// Optional build configuration: "Debug" or "Release" (default: "Release").
    #[serde(default)]
    pub config: Option<String>,
    /// Goal run ID (for audit tracking).
    #[serde(default)]
    pub goal_run_id: Option<String>,
}

/// Parameters for `unity_scene_query`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnitySceneQueryParams {
    /// Asset path of the scene to query (e.g. "Assets/Scenes/Main.unity").
    /// Pass an empty string to query the currently-open scene.
    #[serde(default)]
    pub scene_path: String,
    #[serde(default)]
    pub goal_run_id: Option<String>,
}

/// Parameters for `unity_test_run`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnityTestRunParams {
    /// Optional test name filter (substring match). Omit to run all tests.
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub goal_run_id: Option<String>,
}

/// Parameters for `unity_addressables_build`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnityAddressablesBuildParams {
    #[serde(default)]
    pub goal_run_id: Option<String>,
}

/// Parameters for `unity_render_capture`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnityRenderCaptureParams {
    /// GameObject path to the camera (e.g. "/Main Camera").
    pub camera_path: String,
    /// Destination file path for the PNG (relative to Unity project root).
    pub output_path: String,
    #[serde(default)]
    pub goal_run_id: Option<String>,
}

// ── Input validation ──────────────────────────────────────────────────────────

/// Reject values that contain path separators or `..` — these would corrupt
/// the policy URI interpolated from user-supplied parameters.
/// Returns `invalid_parameter` if validation fails.
fn validate_path_component(value: &str, param_name: &str) -> Result<(), McpError> {
    if value.contains('/') || value.contains('\\') || value.contains("..") {
        return Err(McpError::invalid_params(
            format!(
                "invalid_parameter: '{}' must not contain path separators ('/', '\\\\') or '..'. \
                 Got: {:?}. Use simple identifiers such as 'StandaloneOSX' or 'MainCamera'.",
                param_name, value
            ),
            None,
        ));
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn resolve_agent_id(state: &GatewayState, goal_run_id: Option<&str>) -> String {
    if let Some(id) = goal_run_id {
        if let Ok(uuid) = uuid::Uuid::parse_str(id) {
            if let Ok(agent_id) = state.agent_for_goal(uuid) {
                return agent_id;
            }
        }
    }
    state.resolve_agent_id()
}

fn check_unity_policy(
    engine: &PolicyEngine,
    agent_id: &str,
    verb: &str,
    resource: &str,
) -> Result<ta_policy::PolicyDecision, McpError> {
    let request = PolicyRequest {
        agent_id: agent_id.to_string(),
        tool: "unity".to_string(),
        verb: verb.to_string(),
        target_uri: resource.to_string(),
    };
    Ok(engine.evaluate(&request))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub fn handle_unity_build_trigger(
    state: &Arc<Mutex<GatewayState>>,
    params: UnityBuildTriggerParams,
) -> Result<CallToolResult, McpError> {
    let state = state
        .lock()
        .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;

    let agent_id = resolve_agent_id(&state, params.goal_run_id.as_deref());
    validate_path_component(&params.target, "target")?;
    let resource = format!("unity://build/{}", params.target);
    let decision = check_unity_policy(&state.policy_engine, &agent_id, "trigger", &resource)?;
    enforce_policy(&decision)?;

    let response = json!({
        "status": "connector_not_running",
        "message": "Unity MCP server is not reachable. Ensure the Unity Editor is open with com.unity.mcp-server installed.",
        "hint": "Run `ta connector install unity` for setup instructions, or check [connectors.unity] in your config.",
        "target": params.target,
        "config": params.config.as_deref().unwrap_or("Release"),
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&response).unwrap_or_default(),
    )]))
}

pub fn handle_unity_scene_query(
    state: &Arc<Mutex<GatewayState>>,
    params: UnitySceneQueryParams,
) -> Result<CallToolResult, McpError> {
    let state = state
        .lock()
        .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;

    let agent_id = resolve_agent_id(&state, params.goal_run_id.as_deref());
    let scene = if params.scene_path.is_empty() {
        "active".to_string()
    } else {
        params.scene_path.clone()
    };
    // TODO(v0.15.x): scene_path legitimately contains '/' (e.g. "Assets/Scenes/Main.unity")
    // so validate_path_component cannot be applied here as-is. A path-aware validator
    // (allowlist of safe characters, no '..', no absolute roots) is tracked as a future item.
    let resource = format!("unity://scene/{}", scene);
    let decision = check_unity_policy(&state.policy_engine, &agent_id, "read", &resource)?;
    enforce_policy(&decision)?;

    let response = json!({
        "status": "connector_not_running",
        "message": "Unity MCP server is not reachable.",
        "hint": "Ensure the Unity Editor is open and com.unity.mcp-server is installed. Check [connectors.unity] socket in config.",
        "scene_path": params.scene_path,
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&response).unwrap_or_default(),
    )]))
}

pub fn handle_unity_test_run(
    state: &Arc<Mutex<GatewayState>>,
    params: UnityTestRunParams,
) -> Result<CallToolResult, McpError> {
    let state = state
        .lock()
        .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;

    let agent_id = resolve_agent_id(&state, params.goal_run_id.as_deref());
    let resource = "unity://test/run";
    let decision = check_unity_policy(&state.policy_engine, &agent_id, "run", resource)?;
    enforce_policy(&decision)?;

    let response = json!({
        "status": "connector_not_running",
        "message": "Unity MCP server is not reachable.",
        "hint": "Ensure the Unity Editor is open with com.unity.mcp-server installed.",
        "filter": params.filter,
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&response).unwrap_or_default(),
    )]))
}

pub fn handle_unity_addressables_build(
    state: &Arc<Mutex<GatewayState>>,
    params: UnityAddressablesBuildParams,
) -> Result<CallToolResult, McpError> {
    let state = state
        .lock()
        .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;

    let agent_id = resolve_agent_id(&state, params.goal_run_id.as_deref());
    let resource = "unity://build/addressables";
    let decision = check_unity_policy(&state.policy_engine, &agent_id, "trigger", resource)?;
    enforce_policy(&decision)?;

    let response = json!({
        "status": "connector_not_running",
        "message": "Unity MCP server is not reachable.",
        "hint": "Ensure the Unity Editor is open with com.unity.mcp-server installed and Addressables package configured.",
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&response).unwrap_or_default(),
    )]))
}

pub fn handle_unity_render_capture(
    state: &Arc<Mutex<GatewayState>>,
    params: UnityRenderCaptureParams,
) -> Result<CallToolResult, McpError> {
    let state = state
        .lock()
        .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;

    let agent_id = resolve_agent_id(&state, params.goal_run_id.as_deref());
    validate_path_component(&params.camera_path, "camera_path")?;
    let resource = format!("unity://render/capture/{}", params.camera_path);
    let decision = check_unity_policy(&state.policy_engine, &agent_id, "capture", &resource)?;
    enforce_policy(&decision)?;

    let response = json!({
        "status": "connector_not_running",
        "message": "Unity MCP server is not reachable.",
        "hint": "Ensure the Unity Editor is open with com.unity.mcp-server installed.",
        "camera_path": params.camera_path,
        "output_path": params.output_path,
    });

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&response).unwrap_or_default(),
    )]))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    use crate::config::GatewayConfig;
    use crate::server::GatewayState;

    fn make_state(dir: &std::path::Path) -> Arc<Mutex<GatewayState>> {
        use chrono::{Duration, Utc};
        use ta_policy::{CapabilityGrant, CapabilityManifest};
        use uuid::Uuid;

        let config = GatewayConfig::for_project(dir);
        let state = GatewayState::new(config).expect("state init failed");
        let state = Arc::new(Mutex::new(state));

        // Load a capability manifest for the fallback "unknown" agent so that
        // policy checks pass in unit tests (no running goal session required).
        let manifest = CapabilityManifest {
            manifest_id: Uuid::new_v4(),
            agent_id: "unknown".to_string(),
            grants: vec![
                CapabilityGrant {
                    tool: "unity".to_string(),
                    verb: "trigger".to_string(),
                    resource_pattern: "unity://build/**".to_string(),
                },
                CapabilityGrant {
                    tool: "unity".to_string(),
                    verb: "read".to_string(),
                    resource_pattern: "unity://scene/**".to_string(),
                },
                CapabilityGrant {
                    tool: "unity".to_string(),
                    verb: "run".to_string(),
                    resource_pattern: "unity://test/**".to_string(),
                },
                CapabilityGrant {
                    tool: "unity".to_string(),
                    verb: "capture".to_string(),
                    resource_pattern: "unity://render/**".to_string(),
                },
            ],
            issued_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        state.lock().unwrap().policy_engine.load_manifest(manifest);

        state
    }

    /// Extract the JSON body from the first content item of a CallToolResult.
    fn result_json(result: &CallToolResult) -> serde_json::Value {
        let text = serde_json::to_string(&result.content[0]).unwrap();
        // The content is serialised as {"type":"text","text":"<json>"}.
        // Pull the inner "text" string, then parse that as JSON.
        let wrapper: serde_json::Value = serde_json::from_str(&text).unwrap();
        let inner = wrapper["text"].as_str().unwrap_or("");
        serde_json::from_str(inner).unwrap_or_else(|_| serde_json::Value::String(inner.into()))
    }

    // ── Handler stub tests (item 3) ───────────────────────────────────────────

    #[test]
    fn unity_build_trigger_returns_connector_not_running() {
        let dir = tempdir().unwrap();
        let state = make_state(dir.path());
        let params = UnityBuildTriggerParams {
            target: "StandaloneOSX".into(),
            config: None,
            goal_run_id: None,
        };
        let result = handle_unity_build_trigger(&state, params).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let body = result_json(&result);
        assert_eq!(body["status"], "connector_not_running");
        assert_eq!(body["target"], "StandaloneOSX");
    }

    #[test]
    fn unity_scene_query_returns_connector_not_running() {
        let dir = tempdir().unwrap();
        let state = make_state(dir.path());
        let params = UnitySceneQueryParams {
            scene_path: "Assets/Scenes/Main.unity".into(),
            goal_run_id: None,
        };
        let result = handle_unity_scene_query(&state, params).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let body = result_json(&result);
        assert_eq!(body["status"], "connector_not_running");
    }

    #[test]
    fn unity_test_run_returns_connector_not_running() {
        let dir = tempdir().unwrap();
        let state = make_state(dir.path());
        let params = UnityTestRunParams {
            filter: None,
            goal_run_id: None,
        };
        let result = handle_unity_test_run(&state, params).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let body = result_json(&result);
        assert_eq!(body["status"], "connector_not_running");
    }

    #[test]
    fn unity_addressables_build_returns_connector_not_running() {
        let dir = tempdir().unwrap();
        let state = make_state(dir.path());
        let params = UnityAddressablesBuildParams { goal_run_id: None };
        let result = handle_unity_addressables_build(&state, params).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let body = result_json(&result);
        assert_eq!(body["status"], "connector_not_running");
    }

    #[test]
    fn unity_render_capture_returns_connector_not_running() {
        let dir = tempdir().unwrap();
        let state = make_state(dir.path());
        let params = UnityRenderCaptureParams {
            camera_path: "MainCamera".into(),
            output_path: "Screenshots/frame.png".into(),
            goal_run_id: None,
        };
        let result = handle_unity_render_capture(&state, params).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let body = result_json(&result);
        assert_eq!(body["status"], "connector_not_running");
        assert_eq!(body["camera_path"], "MainCamera");
    }

    // ── Input sanitisation tests (item 1) ────────────────────────────────────

    #[test]
    fn build_trigger_rejects_path_traversal_in_target() {
        let dir = tempdir().unwrap();
        let state = make_state(dir.path());
        let params = UnityBuildTriggerParams {
            target: "StandaloneOSX/../render/capture/foo".into(),
            config: None,
            goal_run_id: None,
        };
        let err =
            handle_unity_build_trigger(&state, params).expect_err("should reject path traversal");
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("invalid_parameter"),
            "error should mention invalid_parameter: {}",
            msg
        );
    }

    #[test]
    fn build_trigger_accepts_valid_target() {
        let dir = tempdir().unwrap();
        let state = make_state(dir.path());
        let params = UnityBuildTriggerParams {
            target: "StandaloneOSX".into(),
            config: Some("Release".into()),
            goal_run_id: None,
        };
        let result = handle_unity_build_trigger(&state, params).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let body = result_json(&result);
        assert_eq!(body["status"], "connector_not_running");
    }
}
