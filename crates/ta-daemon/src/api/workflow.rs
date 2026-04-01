// workflow.rs — Workflow management API (v0.14.20).
//
// GET  /api/workflows              — list workflow TOML files from .ta/workflows/
// POST /api/workflow/:id/input     — human decision for paused workflow
// POST /api/workflow/generate      — generate workflow TOML from a plain-English description
// POST /api/workflow/save          — save (create/update) a workflow TOML file

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::AppState;

/// Workflow list entry.
#[derive(Debug, Serialize)]
pub struct WorkflowEntry {
    pub id: String,
    pub name: String,
    /// "scheduled" | "manual" | "unknown"
    pub trigger: String,
    /// "running" | "idle" | "unknown"
    pub status: String,
    pub toml_path: String,
}

/// Request body for workflow interaction.
#[derive(Debug, Deserialize)]
pub struct WorkflowInputRequest {
    pub decision: String,
    #[serde(default)]
    pub feedback: Option<String>,
}

/// Request body for workflow generation.
#[derive(Debug, Deserialize)]
pub struct WorkflowGenerateRequest {
    pub description: String,
}

/// Request body for workflow save.
#[derive(Debug, Deserialize)]
pub struct WorkflowSaveRequest {
    pub id: String,
    pub toml: String,
}

/// List active workflows from .ta/workflows/.
///
/// GET /api/workflows
pub async fn list_workflows(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let project_root = state.active_project_root.read().unwrap().clone();
    let workflows_dir = project_root.join(".ta").join("workflows");

    let mut entries: Vec<WorkflowEntry> = Vec::new();

    if let Ok(dir) = std::fs::read_dir(&workflows_dir) {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let toml_path = path.display().to_string();
            let text = std::fs::read_to_string(&path).unwrap_or_default();

            // Simple heuristic: check for [trigger] section.
            let trigger = if text.contains("[trigger]") || text.contains("cron") {
                "scheduled"
            } else {
                "manual"
            };

            entries.push(WorkflowEntry {
                name: id
                    .replace('-', " ")
                    .split_whitespace()
                    .map(|w| {
                        let mut c = w.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().to_string() + c.as_str(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                id,
                trigger: trigger.to_string(),
                status: "idle".to_string(),
                toml_path,
            });
        }
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let count = entries.len();

    Json(serde_json::json!({
        "workflows": entries,
        "count": count,
    }))
    .into_response()
}

/// Handle human input for a paused workflow.
///
/// POST /api/workflow/:id/input
pub async fn workflow_input(
    State(_state): State<Arc<AppState>>,
    Path(workflow_id): Path<String>,
    Json(request): Json<WorkflowInputRequest>,
) -> impl IntoResponse {
    let valid_decisions = ["proceed", "revise", "cancel"];
    if !valid_decisions.contains(&request.decision.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "Invalid decision: '{}'. Valid decisions: proceed, revise, cancel",
                    request.decision
                ),
            })),
        )
            .into_response();
    }

    tracing::info!(
        workflow_id = %workflow_id,
        decision = %request.decision,
        feedback = ?request.feedback,
        "workflow input received"
    );

    Json(serde_json::json!({
        "workflow_id": workflow_id,
        "decision": request.decision,
        "status": if request.decision == "cancel" { "cancelled" } else { "acknowledged" },
        "message": format!(
            "Decision '{}' recorded for workflow {}.",
            request.decision,
            &workflow_id[..8.min(workflow_id.len())]
        ),
    }))
    .into_response()
}

/// Generate a workflow TOML from a plain-English description.
///
/// POST /api/workflow/generate
/// Body: { description: "check inbox every 30 min and draft replies" }
pub async fn generate_workflow(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<WorkflowGenerateRequest>,
) -> impl IntoResponse {
    if body.description.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "description is required"})),
        )
            .into_response();
    }

    // Generate a workflow TOML template based on the description.
    // In a full implementation this would call an LLM agent. For now we produce
    // a well-structured template that the user can edit.
    let slug = body
        .description
        .to_lowercase()
        .split_whitespace()
        .take(5)
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    let workflow_id = if slug.is_empty() {
        "new-workflow".to_string()
    } else {
        slug
    };

    let toml = format!(
        r#"# Workflow: {}
# Generated from description: {}

[workflow]
name = "{}"
description = """
{}
"""

[trigger]
# type = "schedule"   # Uncomment and set cron for scheduled execution.
# cron = "0 * * * *"  # Every hour. See https://crontab.guru/
type = "manual"

[[steps]]
name = "main"
goal = "{}"
agent = "claude-code"
"#,
        workflow_id, body.description, workflow_id, body.description, body.description,
    );

    Json(serde_json::json!({
        "id": workflow_id,
        "toml": toml,
        "message": "Review and edit the generated workflow below, then save it.",
    }))
    .into_response()
}

/// Save (create or update) a workflow TOML file.
///
/// POST /api/workflow/save
pub async fn save_workflow(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WorkflowSaveRequest>,
) -> impl IntoResponse {
    if body.id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "id is required"})),
        )
            .into_response();
    }

    // Sanitise the ID to prevent path traversal.
    let id = body
        .id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();

    if id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "id contains no valid characters"})),
        )
            .into_response();
    }

    let project_root = state.active_project_root.read().unwrap().clone();
    let workflows_dir = project_root.join(".ta").join("workflows");
    if let Err(e) = std::fs::create_dir_all(&workflows_dir) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Could not create workflows dir: {}", e)})),
        )
            .into_response();
    }

    let path = workflows_dir.join(format!("{}.toml", id));
    if let Err(e) = std::fs::write(&path, &body.toml) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Could not write workflow: {}", e)})),
        )
            .into_response();
    }

    Json(serde_json::json!({
        "ok": true,
        "id": id,
        "path": path.display().to_string(),
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_entry_is_serializable() {
        let entry = WorkflowEntry {
            id: "test-wf".into(),
            name: "Test Wf".into(),
            trigger: "manual".into(),
            status: "idle".into(),
            toml_path: "/tmp/test-wf.toml".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test-wf"));
    }
}
