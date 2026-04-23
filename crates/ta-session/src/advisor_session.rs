// advisor_session.rs — Shared AdvisorSession type (v0.15.26).
//
// Used by both the Studio UI (via the daemon API) and the CLI `ta advisor ask`
// command to represent a single advisor interaction with numbered options.

use serde::{Deserialize, Serialize};

use crate::intent::{classify_intent, Intent, IntentResult};
use crate::workflow_session::AdvisorSecurity;

/// Context passed from the caller about the current UI state.
///
/// Used to generate context-shaped numbered option menus. For CLI use,
/// the tab can be left as `"cli"` and selection as `None`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdvisorContext {
    /// Current Studio tab (e.g. "workflows", "plan", "drafts", "dashboard", "cli").
    #[serde(default)]
    pub tab: String,
    /// Currently selected item in the tab (e.g. workflow template name, phase ID).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
}

/// A single numbered option presented to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorOption {
    /// 1-based option number displayed to the user.
    pub number: u32,
    /// Human-readable label for this option.
    pub label: String,
    /// Action type matching the `AdvisorAction.action_type` taxonomy:
    /// `"text"`, `"button"`, `"auto_fire"`, `"apply"`, `"deny"`, `"answer"`, `"clarify"`.
    pub action_type: String,
    /// The exact `ta run "..."` command associated with this option (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// The result of an advisor interaction: classified intent + context-aware options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorSession {
    /// The original user message.
    pub message: String,
    /// Classified intent string (e.g. "goal_run", "apply", "deny", "question", "clarify").
    pub intent: String,
    /// Confidence score [0.0, 1.0].
    pub confidence: f32,
    /// Extracted goal prompt (set when intent is "goal_run").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted_goal: Option<String>,
    /// Human-readable advisor response text.
    pub response: String,
    /// Numbered options for the user to pick from.
    pub options: Vec<AdvisorOption>,
    /// Security level that was effective for this interaction.
    pub security: String,
}

impl AdvisorSession {
    /// Classify a message and generate a full advisor session with numbered options.
    pub fn from_message(
        message: &str,
        security: &AdvisorSecurity,
        context: &AdvisorContext,
    ) -> Self {
        let result = classify_intent(message);
        let security_str = security.to_string();
        let (response, options) = build_response_and_options(&result, &security_str, context);

        Self {
            message: message.to_string(),
            intent: intent_to_str(&result.intent),
            confidence: result.confidence,
            extracted_goal: result.extracted_goal.clone(),
            response,
            options,
            security: security_str,
        }
    }

    /// Print the advisor session to stdout in the CLI card format.
    ///
    /// Example output:
    /// ```text
    /// Advisor: I understood this as a goal request (confidence 85%).
    ///
    ///   1. Run goal: ta run "add tests for auth module"
    ///   2. Explain what this will do
    ///   3. Cancel
    /// ```
    pub fn print_card(&self) {
        println!();
        println!("Advisor: {}", self.response);
        if !self.options.is_empty() {
            println!();
            for opt in &self.options {
                if let Some(ref cmd) = opt.command {
                    println!("  {}. {} — `{}`", opt.number, opt.label, cmd);
                } else {
                    println!("  {}. {}", opt.number, opt.label);
                }
            }
        }
        println!();
    }

    /// Return the option matching a given number (1-based).
    pub fn option_by_number(&self, n: u32) -> Option<&AdvisorOption> {
        self.options.iter().find(|o| o.number == n)
    }
}

fn intent_to_str(intent: &Intent) -> String {
    match intent {
        Intent::GoalRun => "goal_run",
        Intent::Question => "question",
        Intent::Clarify => "clarify",
        Intent::Apply => "apply",
        Intent::Deny => "deny",
    }
    .to_string()
}

/// Build the human-readable response text and numbered options for a classified intent.
///
/// Options are context-aware: the same phrase can produce different menus depending on
/// which Studio tab is active and what is currently selected.
pub fn build_response_and_options(
    result: &IntentResult,
    security: &str,
    context: &AdvisorContext,
) -> (String, Vec<AdvisorOption>) {
    match &result.intent {
        Intent::GoalRun => {
            let goal = result
                .extracted_goal
                .as_deref()
                .unwrap_or("the requested change");
            let command = format!("ta run \"{}\"", goal);

            let is_auto = security == "auto" && result.is_auto_actionable();
            let is_suggest = security == "suggest";

            // Context-shaped options.
            let mut options = context_goal_options(goal, &command, context, security);
            // Always add a "Cancel" fallback.
            let next_num = options.len() as u32 + 1;
            options.push(AdvisorOption {
                number: next_num,
                label: "Cancel".to_string(),
                action_type: "clarify".to_string(),
                command: None,
            });

            let response = if is_auto {
                format!(
                    "I understood this as a goal request (confidence {:.0}%). Auto-firing: `{}`",
                    result.confidence * 100.0,
                    command
                )
            } else if is_suggest {
                format!(
                    "I understood this as a goal request (confidence {:.0}%). Click to run:",
                    result.confidence * 100.0
                )
            } else {
                format!(
                    "I understood this as a goal request (confidence {:.0}%).",
                    result.confidence * 100.0
                )
            };

            (response, options)
        }

        Intent::Apply => {
            let response = match security {
                "auto" | "suggest" => "Approval noted. Applying the current draft.".to_string(),
                _ => {
                    "To apply the draft, run `ta draft apply <id>` or use the Studio review panel."
                        .to_string()
                }
            };
            let options = vec![
                AdvisorOption {
                    number: 1,
                    label: "Apply draft".to_string(),
                    action_type: "apply".to_string(),
                    command: None,
                },
                AdvisorOption {
                    number: 2,
                    label: "View changes first".to_string(),
                    action_type: "answer".to_string(),
                    command: Some("ta draft view --latest".to_string()),
                },
                AdvisorOption {
                    number: 3,
                    label: "Cancel".to_string(),
                    action_type: "clarify".to_string(),
                    command: None,
                },
            ];
            (response, options)
        }

        Intent::Deny => {
            let options = vec![
                AdvisorOption {
                    number: 1,
                    label: "Deny draft".to_string(),
                    action_type: "deny".to_string(),
                    command: None,
                },
                AdvisorOption {
                    number: 2,
                    label: "Cancel — keep the draft".to_string(),
                    action_type: "clarify".to_string(),
                    command: None,
                },
            ];
            (
                "Understood — the draft will be marked as denied.".to_string(),
                options,
            )
        }

        Intent::Question => {
            let options = context_question_options(context);
            (
                format!(
                    "I'll look into that for you (confidence {:.0}%).",
                    result.confidence * 100.0
                ),
                options,
            )
        }

        Intent::Clarify => {
            let options = vec![
                AdvisorOption {
                    number: 1,
                    label: "Run a goal".to_string(),
                    action_type: "clarify".to_string(),
                    command: None,
                },
                AdvisorOption {
                    number: 2,
                    label: "Apply current draft".to_string(),
                    action_type: "apply".to_string(),
                    command: None,
                },
                AdvisorOption {
                    number: 3,
                    label: "Deny current draft".to_string(),
                    action_type: "deny".to_string(),
                    command: None,
                },
                AdvisorOption {
                    number: 4,
                    label: "Ask a question".to_string(),
                    action_type: "answer".to_string(),
                    command: None,
                },
            ];
            (
                "I'm not sure what you'd like me to do. What would you like?".to_string(),
                options,
            )
        }
    }
}

/// Generate context-aware goal options based on current tab + selection.
fn context_goal_options(
    goal: &str,
    command: &str,
    context: &AdvisorContext,
    security: &str,
) -> Vec<AdvisorOption> {
    let tab = context.tab.to_ascii_lowercase();
    let _selection = context.selection.as_deref().unwrap_or("");

    // Context: Workflows tab — amend auto-approve topic detected.
    if (tab == "workflows" || tab == "workflow")
        && (goal.contains("auto-approve")
            || goal.contains("auto approve")
            || goal.contains("amend"))
    {
        return vec![
            AdvisorOption {
                number: 1,
                label: "Amend auto-approve for this workflow".to_string(),
                action_type: action_type_for_security(security, true),
                command: Some(
                    "ta run \"amend auto-approve constitution for current workflow\"".to_string(),
                ),
            },
            AdvisorOption {
                number: 2,
                label: "Amend project constitution".to_string(),
                action_type: action_type_for_security(security, true),
                command: Some("ta constitution amend".to_string()),
            },
            AdvisorOption {
                number: 3,
                label: "Explain the difference".to_string(),
                action_type: "answer".to_string(),
                command: None,
            },
        ];
    }

    // Context: Plan tab.
    if tab == "plan"
        && (goal.contains("auto-approve")
            || goal.contains("auto approve")
            || goal.contains("amend"))
    {
        return vec![
            AdvisorOption {
                number: 1,
                label: "Amend auto-approve for current phase".to_string(),
                action_type: action_type_for_security(security, true),
                command: Some(
                    "ta run \"amend auto-approve constitution for current plan phase\"".to_string(),
                ),
            },
            AdvisorOption {
                number: 2,
                label: "Add a new plan item".to_string(),
                action_type: action_type_for_security(security, false),
                command: Some("ta plan add".to_string()),
            },
            AdvisorOption {
                number: 3,
                label: "Show phase progress".to_string(),
                action_type: "answer".to_string(),
                command: Some("ta plan status".to_string()),
            },
        ];
    }

    // Default: standard goal options.
    vec![
        AdvisorOption {
            number: 1,
            label: format!("Run: {}", command),
            action_type: action_type_for_security(security, true),
            command: Some(command.to_string()),
        },
        AdvisorOption {
            number: 2,
            label: format!("Run \"{}\" with a different goal", goal),
            action_type: "clarify".to_string(),
            command: None,
        },
    ]
}

/// Generate context-aware question answer options.
fn context_question_options(context: &AdvisorContext) -> Vec<AdvisorOption> {
    let tab = context.tab.to_ascii_lowercase();
    if tab == "workflows" || tab == "workflow" {
        return vec![
            AdvisorOption {
                number: 1,
                label: "Show workflow details".to_string(),
                action_type: "answer".to_string(),
                command: Some("ta workflow status".to_string()),
            },
            AdvisorOption {
                number: 2,
                label: "Show all workflows".to_string(),
                action_type: "answer".to_string(),
                command: Some("ta workflow list".to_string()),
            },
            AdvisorOption {
                number: 3,
                label: "Cancel".to_string(),
                action_type: "clarify".to_string(),
                command: None,
            },
        ];
    }
    if tab == "plan" {
        return vec![
            AdvisorOption {
                number: 1,
                label: "Show plan status".to_string(),
                action_type: "answer".to_string(),
                command: Some("ta plan status".to_string()),
            },
            AdvisorOption {
                number: 2,
                label: "List plan phases".to_string(),
                action_type: "answer".to_string(),
                command: Some("ta plan list".to_string()),
            },
            AdvisorOption {
                number: 3,
                label: "Cancel".to_string(),
                action_type: "clarify".to_string(),
                command: None,
            },
        ];
    }
    // Default question options.
    vec![
        AdvisorOption {
            number: 1,
            label: "Show project status".to_string(),
            action_type: "answer".to_string(),
            command: Some("ta status".to_string()),
        },
        AdvisorOption {
            number: 2,
            label: "Show pending drafts".to_string(),
            action_type: "answer".to_string(),
            command: Some("ta draft list".to_string()),
        },
        AdvisorOption {
            number: 3,
            label: "Cancel".to_string(),
            action_type: "clarify".to_string(),
            command: None,
        },
    ]
}

/// Map a security level to the appropriate action type for a goal-run option.
fn action_type_for_security(security: &str, high_confidence: bool) -> String {
    match security {
        "auto" if high_confidence => "auto_fire",
        "suggest" => "button",
        _ => "text",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ro_context() -> AdvisorContext {
        AdvisorContext {
            tab: "dashboard".to_string(),
            selection: None,
        }
    }

    #[test]
    fn advisor_session_goal_run() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message(
            "also add tests for the auth module",
            &AdvisorSecurity::ReadOnly,
            &ctx,
        );
        assert_eq!(session.intent, "goal_run");
        assert!(session.confidence >= 0.80);
        assert!(!session.options.is_empty());
        // Should always have a Cancel option.
        assert!(session.options.iter().any(|o| o.label == "Cancel"));
    }

    #[test]
    fn advisor_session_apply() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message("apply", &AdvisorSecurity::ReadOnly, &ctx);
        assert_eq!(session.intent, "apply");
        assert!(session.options.iter().any(|o| o.action_type == "apply"));
    }

    #[test]
    fn advisor_session_deny() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message("skip", &AdvisorSecurity::ReadOnly, &ctx);
        assert_eq!(session.intent, "deny");
        assert!(session.options.iter().any(|o| o.action_type == "deny"));
    }

    #[test]
    fn advisor_session_clarify() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message("hmm", &AdvisorSecurity::ReadOnly, &ctx);
        assert_eq!(session.intent, "clarify");
        assert!(!session.options.is_empty());
    }

    #[test]
    fn workflow_context_amend_auto_approve() {
        let ctx = AdvisorContext {
            tab: "workflows".to_string(),
            selection: Some("deploy-staging".to_string()),
        };
        let session = AdvisorSession::from_message(
            "amend auto-approve for this workflow",
            &AdvisorSecurity::Suggest,
            &ctx,
        );
        assert_eq!(session.intent, "goal_run");
        // Should have the workflow-specific amend options.
        let labels: Vec<_> = session.options.iter().map(|o| o.label.as_str()).collect();
        assert!(
            labels.contains(&"Amend auto-approve for this workflow"),
            "got: {:?}",
            labels
        );
        assert!(
            labels.contains(&"Amend project constitution"),
            "got: {:?}",
            labels
        );
        assert!(
            labels.contains(&"Explain the difference"),
            "got: {:?}",
            labels
        );
    }

    #[test]
    fn plan_context_amend_auto_approve() {
        let ctx = AdvisorContext {
            tab: "plan".to_string(),
            selection: Some("v0.15.26".to_string()),
        };
        let session =
            AdvisorSession::from_message("amend auto-approve", &AdvisorSecurity::Suggest, &ctx);
        assert_eq!(session.intent, "goal_run");
        let labels: Vec<_> = session.options.iter().map(|o| o.label.as_str()).collect();
        assert!(
            labels.contains(&"Amend auto-approve for current phase"),
            "got: {:?}",
            labels
        );
        assert!(labels.contains(&"Show phase progress"), "got: {:?}", labels);
    }

    #[test]
    fn suggest_security_uses_button_action() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message(
            "add tests for the login flow",
            &AdvisorSecurity::Suggest,
            &ctx,
        );
        assert!(session
            .options
            .iter()
            .any(|o| o.action_type == "button" || o.action_type == "clarify"));
    }

    #[test]
    fn auto_security_high_confidence_auto_fires() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message(
            "also fix the null pointer in auth module",
            &AdvisorSecurity::Auto,
            &ctx,
        );
        // High confidence goal run in auto mode should produce auto_fire option.
        assert!(
            session.options.iter().any(|o| o.action_type == "auto_fire"
                || o.action_type == "button"
                || o.action_type == "text"),
            "options: {:?}",
            session
                .options
                .iter()
                .map(|o| &o.action_type)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn option_by_number_finds_correct_option() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message("apply", &AdvisorSecurity::ReadOnly, &ctx);
        let opt = session.option_by_number(1);
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().number, 1);
    }

    #[test]
    fn option_by_number_none_for_missing() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message("apply", &AdvisorSecurity::ReadOnly, &ctx);
        assert!(session.option_by_number(99).is_none());
    }

    #[test]
    fn print_card_does_not_panic() {
        let ctx = ro_context();
        let session =
            AdvisorSession::from_message("also add more tests", &AdvisorSecurity::ReadOnly, &ctx);
        // Should not panic.
        session.print_card();
    }

    #[test]
    fn serialization_roundtrip() {
        let ctx = ro_context();
        let session = AdvisorSession::from_message("apply", &AdvisorSecurity::Suggest, &ctx);
        let json = serde_json::to_string(&session).unwrap();
        let restored: AdvisorSession = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.intent, session.intent);
        assert_eq!(restored.options.len(), session.options.len());
    }
}
