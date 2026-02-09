// event.rs — Audit event data model.
//
// Every significant action in the system (tool call, policy decision,
// approval, commit) is recorded as an AuditEvent. Events form a chain:
// each event includes a `previous_hash` linking it to the prior event,
// enabling tamper detection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What kind of action this event records.
///
/// In Rust, an `enum` can carry data in each variant (called a "tagged union"
/// or "algebraic data type"). Here we use simple variants without data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// An MCP tool was called (e.g., fs.read, fs.write_patch).
    ToolCall,
    /// The policy engine made a decision (allow/deny/require_approval).
    PolicyDecision,
    /// A human approved a PR package or action.
    Approval,
    /// Changes were applied to the real target (commit/send/post).
    Apply,
    /// An error occurred during processing.
    Error,
}

/// A single audit event — one line in the JSONL audit log.
///
/// `#[derive(Serialize, Deserialize)]` lets serde automatically convert
/// this struct to/from JSON. Each field maps to a JSON key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique identifier for this event.
    pub event_id: Uuid,

    /// When this event occurred (UTC).
    pub timestamp: DateTime<Utc>,

    /// Which agent performed the action.
    pub agent_id: String,

    /// What kind of action was performed.
    pub action: AuditAction,

    /// The resource affected (e.g., "fs://workspace/src/main.rs").
    /// `Option<T>` means this field can be `Some(value)` or `None` (null in JSON).
    pub target_uri: Option<String>,

    /// SHA-256 hash of the input to this action.
    pub input_hash: Option<String>,

    /// SHA-256 hash of the output/result of this action.
    pub output_hash: Option<String>,

    /// Links this event to a parent event (for causal chaining).
    pub parent_event_id: Option<Uuid>,

    /// Hash of the previous event in the log (for tamper detection).
    /// The first event in the log has this set to None.
    pub previous_hash: Option<String>,

    /// Arbitrary additional data. `serde_json::Value` can hold any JSON.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl AuditEvent {
    /// Create a new audit event with the current timestamp and a random UUID.
    ///
    /// Most fields start as None — set them before logging.
    pub fn new(agent_id: impl Into<String>, action: AuditAction) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            agent_id: agent_id.into(),
            action,
            target_uri: None,
            input_hash: None,
            output_hash: None,
            parent_event_id: None,
            previous_hash: None,
            metadata: serde_json::Value::Null,
        }
    }

    /// Set the target URI and return self (builder pattern).
    ///
    /// Builder pattern lets you chain calls:
    ///   `AuditEvent::new("agent-1", ToolCall).with_target("fs://...")`
    pub fn with_target(mut self, uri: impl Into<String>) -> Self {
        self.target_uri = Some(uri.into());
        self
    }

    /// Set the input hash and return self.
    pub fn with_input_hash(mut self, hash: impl Into<String>) -> Self {
        self.input_hash = Some(hash.into());
        self
    }

    /// Set the output hash and return self.
    pub fn with_output_hash(mut self, hash: impl Into<String>) -> Self {
        self.output_hash = Some(hash.into());
        self
    }

    /// Set the parent event ID and return self.
    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_event_id = Some(parent_id);
        self
    }

    /// Set arbitrary metadata and return self.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serialization_round_trip() {
        // Create an event, serialize to JSON, deserialize back, compare.
        let event = AuditEvent::new("test-agent", AuditAction::ToolCall)
            .with_target("fs://workspace/test.txt")
            .with_input_hash("abc123")
            .with_output_hash("def456");

        // Serialize to JSON string
        let json = serde_json::to_string(&event).expect("serialize");
        // Deserialize back to struct
        let restored: AuditEvent = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(event.event_id, restored.event_id);
        assert_eq!(event.agent_id, restored.agent_id);
        assert_eq!(event.action, restored.action);
        assert_eq!(event.target_uri, restored.target_uri);
        assert_eq!(event.input_hash, restored.input_hash);
        assert_eq!(event.output_hash, restored.output_hash);
    }

    #[test]
    fn event_ids_are_unique() {
        let e1 = AuditEvent::new("agent", AuditAction::ToolCall);
        let e2 = AuditEvent::new("agent", AuditAction::ToolCall);
        assert_ne!(e1.event_id, e2.event_id);
    }

    #[test]
    fn action_serializes_as_snake_case() {
        // Verify that enum variants serialize as snake_case strings, not PascalCase.
        let json = serde_json::to_string(&AuditAction::PolicyDecision).unwrap();
        assert_eq!(json, "\"policy_decision\"");
    }
}
