// mediator.rs — ResourceMediator trait and core types.
//
// The central abstraction of Layer 1: every state-changing action an agent
// proposes is staged before it touches the real world. The ResourceMediator
// trait generalizes this pattern from files to any resource.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::MediationError;

/// What the agent wants to do — a proposed action on a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    /// Unique identifier for this proposed action.
    pub action_id: Uuid,
    /// URI scheme: "fs", "email", "db", "api", etc.
    pub scheme: String,
    /// Action verb: "write", "send", "delete", "execute", etc.
    pub verb: String,
    /// Target resource URI: "fs://workspace/src/main.rs", "email://draft/123", etc.
    pub target_uri: String,
    /// Action-specific parameters (tool call arguments, file content, etc.).
    pub parameters: serde_json::Value,
    /// When the action was proposed.
    pub proposed_at: DateTime<Utc>,
}

impl ProposedAction {
    /// Create a new proposed action.
    pub fn new(scheme: &str, verb: &str, target_uri: &str) -> Self {
        Self {
            action_id: Uuid::new_v4(),
            scheme: scheme.to_string(),
            verb: verb.to_string(),
            target_uri: target_uri.to_string(),
            parameters: serde_json::Value::Null,
            proposed_at: Utc::now(),
        }
    }

    /// Attach parameters to the action.
    pub fn with_parameters(mut self, params: serde_json::Value) -> Self {
        self.parameters = params;
        self
    }
}

/// The staged version of a proposed action — held until approved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedMutation {
    /// Unique identifier for this staged mutation.
    pub mutation_id: Uuid,
    /// The original proposed action.
    pub action: ProposedAction,
    /// When the action was staged.
    pub staged_at: DateTime<Utc>,
    /// Human-readable preview (generated lazily or eagerly).
    pub preview: Option<MutationPreview>,
    /// Where the staged data lives (path, ref, or opaque handle).
    pub staging_ref: String,
}

/// Human-readable preview of a staged mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationPreview {
    /// One-line summary of what will happen.
    pub summary: String,
    /// Diff or detailed description (optional for non-file resources).
    pub diff: Option<String>,
    /// Risk flags detected during staging.
    pub risk_flags: Vec<String>,
    /// How risky is this action?
    pub classification: ActionClassification,
}

/// Risk classification for a proposed action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionClassification {
    /// No state change — safe to auto-approve.
    ReadOnly,
    /// Changes local state that can be reverted.
    StateChanging,
    /// Cannot be undone (e.g., send email, delete production data).
    Irreversible,
    /// Touches systems outside TA's control.
    ExternalSideEffect,
}

impl std::fmt::Display for ActionClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionClassification::ReadOnly => write!(f, "read_only"),
            ActionClassification::StateChanging => write!(f, "state_changing"),
            ActionClassification::Irreversible => write!(f, "irreversible"),
            ActionClassification::ExternalSideEffect => write!(f, "external_side_effect"),
        }
    }
}

/// Result of applying a staged mutation to the real resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    /// The mutation that was applied.
    pub mutation_id: Uuid,
    /// Whether the apply succeeded.
    pub success: bool,
    /// Human-readable message about what happened.
    pub message: String,
    /// When the apply occurred.
    pub applied_at: DateTime<Utc>,
}

/// The core trait: stage, preview, apply, rollback for any resource type.
///
/// Implementations exist per URI scheme:
/// - `FsMediator` for `fs://` (files)
/// - Future: `EmailMediator` for `email://`, `DbMediator` for `db://`, etc.
pub trait ResourceMediator: Send + Sync {
    /// Which URI scheme this mediator handles (e.g., "fs", "email", "db").
    fn scheme(&self) -> &str;

    /// Stage a proposed action — capture the mutation without applying it.
    fn stage(&self, action: ProposedAction) -> Result<StagedMutation, MediationError>;

    /// Generate a human-readable preview of a staged mutation.
    fn preview(&self, staged: &StagedMutation) -> Result<MutationPreview, MediationError>;

    /// Apply a staged mutation to the real resource (after human approval).
    fn apply(&self, staged: &StagedMutation) -> Result<ApplyResult, MediationError>;

    /// Roll back a staged mutation (remove from staging without applying).
    fn rollback(&self, staged: &StagedMutation) -> Result<(), MediationError>;

    /// Classify how risky a proposed action is (used for policy decisions).
    fn classify(&self, action: &ProposedAction) -> ActionClassification;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposed_action_builder() {
        let action = ProposedAction::new("fs", "write", "fs://workspace/test.txt")
            .with_parameters(serde_json::json!({"content": "hello"}));

        assert_eq!(action.scheme, "fs");
        assert_eq!(action.verb, "write");
        assert_eq!(action.target_uri, "fs://workspace/test.txt");
        assert_eq!(action.parameters["content"], "hello");
    }

    #[test]
    fn action_classification_serialization_round_trip() {
        for classification in [
            ActionClassification::ReadOnly,
            ActionClassification::StateChanging,
            ActionClassification::Irreversible,
            ActionClassification::ExternalSideEffect,
        ] {
            let json = serde_json::to_string(&classification).unwrap();
            let restored: ActionClassification = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, classification);
        }
    }

    #[test]
    fn staged_mutation_serialization_round_trip() {
        let action = ProposedAction::new("fs", "write", "fs://workspace/main.rs");
        let staged = StagedMutation {
            mutation_id: Uuid::new_v4(),
            action,
            staged_at: Utc::now(),
            preview: Some(MutationPreview {
                summary: "Write main.rs".to_string(),
                diff: Some("+fn main() {}".to_string()),
                risk_flags: vec![],
                classification: ActionClassification::StateChanging,
            }),
            staging_ref: "/tmp/staging/main.rs".to_string(),
        };

        let json = serde_json::to_string(&staged).unwrap();
        let restored: StagedMutation = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.mutation_id, staged.mutation_id);
        assert!(restored.preview.is_some());
    }

    #[test]
    fn apply_result_serialization() {
        let result = ApplyResult {
            mutation_id: Uuid::new_v4(),
            success: true,
            message: "Applied successfully".to_string(),
            applied_at: Utc::now(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
    }

    #[test]
    fn action_classification_display() {
        assert_eq!(format!("{}", ActionClassification::ReadOnly), "read_only");
        assert_eq!(
            format!("{}", ActionClassification::ExternalSideEffect),
            "external_side_effect"
        );
    }
}
