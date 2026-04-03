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
