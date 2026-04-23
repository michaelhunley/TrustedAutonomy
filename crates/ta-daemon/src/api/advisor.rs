// api/advisor.rs — Studio Advisor API (v0.15.26).
//
// Provides the advisor endpoint for the global intent bar and advisor panel.
// Classifies intent on each message and returns context-aware numbered options.
//
// Endpoints:
//   POST /api/advisor/message  — classify intent and return action + numbered options
//   GET  /api/advisor/tools    — list available tools by security level
//   GET  /api/advisor/config   — return current advisor config

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::AppState;
use ta_session::classify_intent;
use ta_session::workflow_session::AdvisorSecurity;
use ta_session::Intent;
use ta_session::{AdvisorContext, AdvisorOption, AdvisorSession};

// ── Request / Response types ──────────────────────────────────────────────────

/// Request body for `POST /api/advisor/message`.
#[derive(Debug, Deserialize)]
pub struct MessageRequest {
    /// The human's message text.
    pub message: String,
    /// Optional security level override for this request.
    /// Overrides the daemon config for this call only.
    #[serde(default)]
    pub security_override: Option<String>,
    /// Optional Studio context (current tab + selection).
    /// Used to generate context-shaped numbered option menus.
    #[serde(default)]
    pub context: Option<AdvisorContext>,
}

/// The action the Studio UI should take based on the classified intent.
#[derive(Debug, Serialize)]
pub struct AdvisorAction {
    /// Action type:
    /// - `"text"`: show the command as copyable text (read_only mode)
    /// - `"button"`: render as a clickable "Run this" button (suggest mode)
    /// - `"auto_fire"`: advisor determined it should fire — Studio calls /api/goal/start
    /// - `"apply"`: human approved; Studio should apply the current draft
    /// - `"deny"`: human declined; Studio should deny the current draft
    /// - `"answer"`: forward to agent for a question answer
    /// - `"clarify"`: advisor needs more information
    #[serde(rename = "type")]
    pub action_type: String,
    /// Human-readable label for buttons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The exact `ta run "..."` command to show or fire (set for GoalRun intents).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// Response from `POST /api/advisor/message`.
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    /// Classified intent.
    pub intent: String,
    /// Confidence score [0.0, 1.0].
    pub confidence: f32,
    /// Extracted goal prompt for GoalRun intents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_goal: Option<String>,
    /// Action the Studio should take (primary action, for backwards compatibility).
    pub action: AdvisorAction,
    /// Human-readable advisor response text shown in the chat pane.
    pub response: String,
    /// Numbered options for the advisor menu (v0.15.26).
    pub options: Vec<AdvisorOption>,
}

/// Response from `GET /api/advisor/tools`.
#[derive(Debug, Serialize)]
pub struct ToolsResponse {
    pub security: String,
    pub tools: Vec<AdvisorTool>,
}

/// A single tool available to the advisor at the given security level.
#[derive(Debug, Serialize)]
pub struct AdvisorTool {
    pub name: String,
    pub description: String,
    pub read_only: bool,
}

/// Response from `GET /api/advisor/config`.
#[derive(Debug, Serialize)]
pub struct AdvisorConfigResponse {
    /// Current security level.
    pub security: String,
    /// Human-readable description of what the advisor can do.
    pub description: String,
}

// ── Security level resolution ─────────────────────────────────────────────────

/// Resolve the effective security level string from the request (override) or config.
fn resolve_security(state: &AppState, override_str: Option<&str>) -> String {
    override_str
        .unwrap_or(state.daemon_config.shell.advisor.security.as_str())
        .to_string()
}

fn parse_security(s: &str) -> AdvisorSecurity {
    match s {
        "auto" => AdvisorSecurity::Auto,
        "suggest" => AdvisorSecurity::Suggest,
        _ => AdvisorSecurity::ReadOnly,
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /api/advisor/message` — Classify intent and return advisor action + numbered options.
///
/// The advisor is explicitly on the human's side: it interprets their intent,
/// presents commands at the right escalation level, flags risks, and provides
/// context-shaped numbered option menus based on the current Studio tab.
pub async fn handle_message(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MessageRequest>,
) -> impl IntoResponse {
    let message = body.message.trim().to_string();
    if message.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "message is required"})),
        )
            .into_response();
    }

    let security_str = resolve_security(&state, body.security_override.as_deref());
    let security = parse_security(&security_str);
    let context = body.context.unwrap_or_default();

    // Use AdvisorSession for unified intent classification + option generation.
    let session = AdvisorSession::from_message(&message, &security, &context);

    // Build backwards-compatible primary action from the first non-cancel option.
    let primary = session
        .options
        .iter()
        .find(|o| o.action_type != "clarify" || session.options.len() == 1)
        .or_else(|| session.options.first());

    let action = if let Some(opt) = primary {
        AdvisorAction {
            action_type: opt.action_type.clone(),
            label: if opt.action_type == "button" {
                Some(opt.label.clone())
            } else {
                None
            },
            command: opt.command.clone(),
        }
    } else {
        // Fallback: use the classify_intent result directly.
        let result = classify_intent(&message);
        let (legacy_action, _) = build_legacy_action(&result, &security_str);
        legacy_action
    };

    Json(MessageResponse {
        intent: session.intent,
        confidence: session.confidence,
        extracted_goal: session.extracted_goal,
        action,
        response: session.response,
        options: session.options,
    })
    .into_response()
}

/// Build a backwards-compatible AdvisorAction from the classified intent (no options).
fn build_legacy_action(
    result: &ta_session::IntentResult,
    security: &str,
) -> (AdvisorAction, String) {
    match &result.intent {
        Intent::GoalRun => {
            let goal = result
                .extracted_goal
                .as_deref()
                .unwrap_or("the requested change");
            let command = format!("ta run \"{}\"", goal);

            match security {
                "auto" if result.is_auto_actionable() => (
                    AdvisorAction {
                        action_type: "auto_fire".to_string(),
                        label: Some("Run goal".to_string()),
                        command: Some(command.clone()),
                    },
                    format!(
                        "Intent: run a goal (confidence {:.0}%). Firing: `{}`",
                        result.confidence * 100.0,
                        command
                    ),
                ),
                "suggest" => (
                    AdvisorAction {
                        action_type: "button".to_string(),
                        label: Some("Run this goal".to_string()),
                        command: Some(command.clone()),
                    },
                    format!(
                        "I understood this as a goal request. Click the button to run: `{}`",
                        command
                    ),
                ),
                _ => (
                    AdvisorAction {
                        action_type: "text".to_string(),
                        label: None,
                        command: Some(command.clone()),
                    },
                    format!(
                        "I understood this as a goal request. Run this command to proceed:\n```\n{}\n```",
                        command
                    ),
                ),
            }
        }
        Intent::Apply => (
            AdvisorAction {
                action_type: "apply".to_string(),
                label: None,
                command: None,
            },
            "Approval noted. Studio should apply the current draft.".to_string(),
        ),
        Intent::Deny => (
            AdvisorAction {
                action_type: "deny".to_string(),
                label: None,
                command: None,
            },
            "Understood — the draft will be marked as denied.".to_string(),
        ),
        Intent::Question => (
            AdvisorAction {
                action_type: "answer".to_string(),
                label: None,
                command: None,
            },
            format!(
                "I'll look into that for you (confidence {:.0}%).",
                result.confidence * 100.0
            ),
        ),
        Intent::Clarify => (
            AdvisorAction {
                action_type: "clarify".to_string(),
                label: None,
                command: None,
            },
            "I'm not sure what you'd like me to do. Could you be more specific?".to_string(),
        ),
    }
}

/// `GET /api/advisor/tools` — List available tools at the current security level.
pub async fn get_tools(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let security = state.daemon_config.shell.advisor.security.clone();
    let tools = available_tools(&security);
    Json(ToolsResponse { security, tools }).into_response()
}

/// Return the tools available at the given security level.
fn available_tools(security: &str) -> Vec<AdvisorTool> {
    let read_only_tools = vec![
        AdvisorTool {
            name: "ta_draft_view".to_string(),
            description: "View a draft package and its changes".to_string(),
            read_only: true,
        },
        AdvisorTool {
            name: "ta_plan_status".to_string(),
            description: "Show plan phase status and progress".to_string(),
            read_only: true,
        },
        AdvisorTool {
            name: "ta_fs_read".to_string(),
            description: "Read file contents from the workspace".to_string(),
            read_only: true,
        },
    ];

    match security {
        "auto" | "suggest" => {
            let mut tools = read_only_tools;
            tools.push(AdvisorTool {
                name: "ta_goal_start".to_string(),
                description: "Start a new goal run (requires human confirmation in suggest mode)"
                    .to_string(),
                read_only: false,
            });
            tools.push(AdvisorTool {
                name: "ta_draft_list".to_string(),
                description: "List pending drafts awaiting review".to_string(),
                read_only: true,
            });
            tools
        }
        _ => read_only_tools,
    }
}

/// `GET /api/advisor/config` — Return current advisor configuration.
pub async fn get_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let security = &state.daemon_config.shell.advisor.security;
    let description = match security.as_str() {
        "auto" => "Advisor may fire goals automatically at ≥80% intent confidence.",
        "suggest" => "Advisor presents goal commands as clickable buttons for human confirmation.",
        _ => "Advisor answers questions and shows commands as copyable text only.",
    };
    Json(AdvisorConfigResponse {
        security: security.clone(),
        description: description.to_string(),
    })
    .into_response()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ta_session::IntentResult;

    fn make_goal_result(confidence: f32) -> ta_session::IntentResult {
        IntentResult::new(Intent::GoalRun, confidence).with_goal("add tests for the auth module")
    }

    #[test]
    fn read_only_goal_run_returns_text_action() {
        let result = make_goal_result(0.85);
        let (action, response) = build_legacy_action(&result, "read_only");
        assert_eq!(action.action_type, "text");
        assert!(action.command.as_deref().unwrap().starts_with("ta run"));
        assert!(response.contains("ta run"));
        assert!(action.label.is_none());
    }

    #[test]
    fn suggest_goal_run_returns_button_action() {
        let result = make_goal_result(0.85);
        let (action, response) = build_legacy_action(&result, "suggest");
        assert_eq!(action.action_type, "button");
        assert_eq!(action.label.as_deref(), Some("Run this goal"));
        assert!(response.contains("Click the button"));
        assert!(action.command.is_some());
    }

    #[test]
    fn auto_high_confidence_returns_auto_fire() {
        let result = make_goal_result(0.85);
        let (action, response) = build_legacy_action(&result, "auto");
        assert_eq!(action.action_type, "auto_fire");
        assert!(response.contains("Firing"));
        assert!(action.command.is_some());
    }

    #[test]
    fn auto_low_confidence_falls_back_to_text() {
        let result = IntentResult::new(Intent::GoalRun, 0.70).with_goal("some vague request");
        let (action, _) = build_legacy_action(&result, "auto");
        assert_eq!(action.action_type, "text");
    }

    #[test]
    fn apply_intent_returns_apply_action() {
        let result = IntentResult::new(Intent::Apply, 0.95);
        let (action, _) = build_legacy_action(&result, "read_only");
        assert_eq!(action.action_type, "apply");
    }

    #[test]
    fn deny_intent_returns_deny_action() {
        let result = IntentResult::new(Intent::Deny, 0.95);
        let (action, _) = build_legacy_action(&result, "read_only");
        assert_eq!(action.action_type, "deny");
    }

    #[test]
    fn question_intent_returns_answer_action() {
        let result = IntentResult::new(Intent::Question, 0.85);
        let (action, _) = build_legacy_action(&result, "read_only");
        assert_eq!(action.action_type, "answer");
    }

    #[test]
    fn clarify_intent_returns_clarify_action() {
        let result = IntentResult::new(Intent::Clarify, 0.50);
        let (action, response) = build_legacy_action(&result, "read_only");
        assert_eq!(action.action_type, "clarify");
        assert!(response.contains("more specific"));
    }

    #[test]
    fn available_tools_read_only_excludes_goal_start() {
        let tools = available_tools("read_only");
        assert!(!tools.iter().any(|t| t.name == "ta_goal_start"));
        assert!(tools.iter().any(|t| t.name == "ta_draft_view"));
        assert!(tools.iter().any(|t| t.name == "ta_plan_status"));
        assert!(tools.iter().any(|t| t.name == "ta_fs_read"));
    }

    #[test]
    fn available_tools_suggest_includes_goal_start() {
        let tools = available_tools("suggest");
        assert!(tools.iter().any(|t| t.name == "ta_goal_start"));
        assert!(tools.iter().any(|t| t.name == "ta_draft_list"));
    }

    #[test]
    fn available_tools_auto_includes_goal_start() {
        let tools = available_tools("auto");
        assert!(tools.iter().any(|t| t.name == "ta_goal_start"));
    }

    #[test]
    fn advisor_session_intent_str_roundtrips() {
        use ta_session::AdvisorContext;
        let ctx = AdvisorContext::default();
        let check = |msg: &str, expected: &str| {
            let s = AdvisorSession::from_message(
                msg,
                &ta_session::workflow_session::AdvisorSecurity::ReadOnly,
                &ctx,
            );
            assert_eq!(s.intent, expected, "message: {}", msg);
        };
        check("also add tests", "goal_run");
        check("what changed?", "question");
        check("apply", "apply");
        check("skip", "deny");
        check("hmm", "clarify");
    }

    #[test]
    fn command_formatted_correctly_for_goal_run() {
        let result = IntentResult::new(Intent::GoalRun, 0.85).with_goal("add tests for login flow");
        let (action, _) = build_legacy_action(&result, "read_only");
        assert_eq!(
            action.command.as_deref(),
            Some("ta run \"add tests for login flow\"")
        );
    }

    #[test]
    fn message_response_includes_options() {
        // Directly test that AdvisorSession produces options.
        use ta_session::{AdvisorContext, AdvisorSession};
        let ctx = AdvisorContext {
            tab: "dashboard".to_string(),
            selection: None,
        };
        let session = AdvisorSession::from_message(
            "also add tests",
            &ta_session::workflow_session::AdvisorSecurity::ReadOnly,
            &ctx,
        );
        assert!(!session.options.is_empty(), "options should not be empty");
        assert!(
            session.options.iter().all(|o| o.number > 0),
            "all options must have a positive number"
        );
    }

    #[test]
    fn context_shapes_workflow_options() {
        use ta_session::{AdvisorContext, AdvisorSession};
        let ctx = AdvisorContext {
            tab: "workflows".to_string(),
            selection: Some("my-workflow".to_string()),
        };
        let session = AdvisorSession::from_message(
            "amend auto-approve",
            &ta_session::workflow_session::AdvisorSecurity::Suggest,
            &ctx,
        );
        let labels: Vec<_> = session.options.iter().map(|o| o.label.as_str()).collect();
        assert!(
            labels.contains(&"Amend auto-approve for this workflow"),
            "got: {:?}",
            labels
        );
    }
}
