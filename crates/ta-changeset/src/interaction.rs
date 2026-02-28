// interaction.rs — Interaction request/response model for ReviewChannel.
//
// These types define the protocol for bidirectional human-agent communication.
// An InteractionRequest is sent when TA needs human input (draft review, plan
// approval, escalation), and InteractionResponse carries the human's decision.
//
// This is the core protocol for v0.4.1.1 (Runtime Channel Architecture).

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What kind of interaction is being requested.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    /// A draft is ready for review — human should approve, reject, or discuss.
    DraftReview,
    /// General approval question (e.g., "proceed with this approach?").
    ApprovalDiscussion,
    /// Agent proposes a plan change — human should accept or reject.
    PlanNegotiation,
    /// Agent is escalating an issue that exceeds its authority.
    Escalation,
    /// Extension point for future interaction types.
    Custom(String),
}

impl fmt::Display for InteractionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InteractionKind::DraftReview => write!(f, "draft_review"),
            InteractionKind::ApprovalDiscussion => write!(f, "approval_discussion"),
            InteractionKind::PlanNegotiation => write!(f, "plan_negotiation"),
            InteractionKind::Escalation => write!(f, "escalation"),
            InteractionKind::Custom(name) => write!(f, "custom:{}", name),
        }
    }
}

/// How urgent is this interaction?
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    /// Agent blocks until human responds.
    Blocking,
    /// Agent can continue, but human should respond eventually.
    Advisory,
    /// Informational only — no response expected.
    Informational,
}

impl fmt::Display for Urgency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Urgency::Blocking => write!(f, "blocking"),
            Urgency::Advisory => write!(f, "advisory"),
            Urgency::Informational => write!(f, "informational"),
        }
    }
}

/// A request from TA to the human, delivered via a ReviewChannel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRequest {
    /// Unique identifier for this interaction (for correlation with response).
    pub interaction_id: Uuid,

    /// What kind of interaction this is.
    pub kind: InteractionKind,

    /// Structured payload — contents depend on `kind`.
    /// For DraftReview: { "draft_id": "...", "summary": "...", "artifact_count": N }
    /// For PlanNegotiation: { "phase": "...", "proposed_status": "..." }
    pub context: serde_json::Value,

    /// How urgent is this interaction?
    pub urgency: Urgency,

    /// Arbitrary key-value pairs for channel-specific rendering hints.
    /// e.g., { "color": "yellow", "thread_id": "..." }
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,

    /// When the request was created.
    pub created_at: DateTime<Utc>,

    /// Optional goal ID for context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<Uuid>,
}

impl InteractionRequest {
    /// Create a new interaction request.
    pub fn new(kind: InteractionKind, context: serde_json::Value, urgency: Urgency) -> Self {
        Self {
            interaction_id: Uuid::new_v4(),
            kind,
            context,
            urgency,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            goal_id: None,
        }
    }

    /// Set the goal ID for this interaction.
    pub fn with_goal_id(mut self, goal_id: Uuid) -> Self {
        self.goal_id = Some(goal_id);
        self
    }

    /// Add a metadata key-value pair.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Create a DraftReview interaction request.
    pub fn draft_review(draft_id: Uuid, summary: &str, artifact_count: usize) -> Self {
        Self::new(
            InteractionKind::DraftReview,
            serde_json::json!({
                "draft_id": draft_id.to_string(),
                "summary": summary,
                "artifact_count": artifact_count,
            }),
            Urgency::Blocking,
        )
    }

    /// Create a PlanNegotiation interaction request.
    pub fn plan_negotiation(phase: &str, proposed_status: &str) -> Self {
        Self::new(
            InteractionKind::PlanNegotiation,
            serde_json::json!({
                "phase": phase,
                "proposed_status": proposed_status,
            }),
            Urgency::Blocking,
        )
    }

    /// Create an Escalation interaction request.
    pub fn escalation(reason: &str, details: serde_json::Value) -> Self {
        Self::new(
            InteractionKind::Escalation,
            serde_json::json!({
                "reason": reason,
                "details": details,
            }),
            Urgency::Blocking,
        )
    }
}

impl fmt::Display for InteractionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} (urgency: {})",
            self.interaction_id, self.kind, self.urgency
        )
    }
}

/// The human's decision in response to an InteractionRequest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum Decision {
    /// Approved — proceed as proposed.
    Approve,
    /// Rejected — do not proceed, with explanation.
    Reject { reason: String },
    /// Human wants to discuss further before deciding.
    Discuss,
    /// Skip this interaction for now (non-blocking interactions only).
    SkipForNow,
}

impl fmt::Display for Decision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Decision::Approve => write!(f, "approved"),
            Decision::Reject { reason } => write!(f, "rejected: {}", reason),
            Decision::Discuss => write!(f, "discuss"),
            Decision::SkipForNow => write!(f, "skipped"),
        }
    }
}

/// The human's response to an InteractionRequest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionResponse {
    /// Correlation ID — must match the InteractionRequest.interaction_id.
    pub interaction_id: Uuid,

    /// The human's decision.
    pub decision: Decision,

    /// Optional free-text reasoning or feedback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,

    /// When the response was created.
    pub responded_at: DateTime<Utc>,

    /// Who responded (channel identity, e.g., "cli:tty0", "slack:U12345").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responder_id: Option<String>,
}

impl InteractionResponse {
    /// Create a new response for a given interaction.
    pub fn new(interaction_id: Uuid, decision: Decision) -> Self {
        Self {
            interaction_id,
            decision,
            reasoning: None,
            responded_at: Utc::now(),
            responder_id: None,
        }
    }

    /// Set reasoning text.
    pub fn with_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.reasoning = Some(reasoning.into());
        self
    }

    /// Set responder identity.
    pub fn with_responder(mut self, responder_id: impl Into<String>) -> Self {
        self.responder_id = Some(responder_id.into());
        self
    }
}

impl fmt::Display for InteractionResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.interaction_id, self.decision)
    }
}

/// A non-blocking notification from TA to the human.
/// Unlike InteractionRequest, no response is expected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Unique notification ID.
    pub notification_id: Uuid,

    /// Human-readable message.
    pub message: String,

    /// Severity level for rendering.
    pub level: NotificationLevel,

    /// When the notification was created.
    pub created_at: DateTime<Utc>,

    /// Optional goal ID for context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<Uuid>,
}

impl Notification {
    /// Create a new notification.
    pub fn new(message: impl Into<String>, level: NotificationLevel) -> Self {
        Self {
            notification_id: Uuid::new_v4(),
            message: message.into(),
            level,
            created_at: Utc::now(),
            goal_id: None,
        }
    }

    /// Set the goal ID.
    pub fn with_goal_id(mut self, goal_id: Uuid) -> Self {
        self.goal_id = Some(goal_id);
        self
    }

    /// Create an info notification.
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, NotificationLevel::Info)
    }

    /// Create a warning notification.
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, NotificationLevel::Warning)
    }
}

/// Notification severity level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl fmt::Display for NotificationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotificationLevel::Debug => write!(f, "debug"),
            NotificationLevel::Info => write!(f, "info"),
            NotificationLevel::Warning => write!(f, "warning"),
            NotificationLevel::Error => write!(f, "error"),
        }
    }
}

/// Describes what a ReviewChannel implementation supports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    /// Whether the channel supports async responses (human responds later, not inline).
    pub supports_async: bool,

    /// Whether the channel supports rich media (images, formatted diffs, etc.).
    pub supports_rich_media: bool,

    /// Whether the channel supports threaded discussions.
    pub supports_threads: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_request_creation() {
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Test draft", 3);
        assert_eq!(req.kind, InteractionKind::DraftReview);
        assert_eq!(req.urgency, Urgency::Blocking);
        assert_eq!(req.context["artifact_count"], 3);
        assert_eq!(req.context["summary"], "Test draft");
    }

    #[test]
    fn interaction_request_with_metadata() {
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Test", 1)
            .with_metadata("color", "yellow")
            .with_goal_id(Uuid::new_v4());
        assert_eq!(req.metadata.get("color").unwrap(), "yellow");
        assert!(req.goal_id.is_some());
    }

    #[test]
    fn plan_negotiation_request() {
        let req = InteractionRequest::plan_negotiation("v0.4.2", "done");
        assert_eq!(req.kind, InteractionKind::PlanNegotiation);
        assert_eq!(req.context["phase"], "v0.4.2");
        assert_eq!(req.context["proposed_status"], "done");
    }

    #[test]
    fn escalation_request() {
        let req = InteractionRequest::escalation(
            "exceeded token budget",
            serde_json::json!({"budget": 10000, "used": 15000}),
        );
        assert_eq!(req.kind, InteractionKind::Escalation);
        assert_eq!(req.context["reason"], "exceeded token budget");
    }

    #[test]
    fn interaction_response_creation() {
        let id = Uuid::new_v4();
        let resp = InteractionResponse::new(id, Decision::Approve)
            .with_reasoning("looks good")
            .with_responder("cli:tty0");
        assert_eq!(resp.interaction_id, id);
        assert_eq!(resp.decision, Decision::Approve);
        assert_eq!(resp.reasoning.as_deref(), Some("looks good"));
        assert_eq!(resp.responder_id.as_deref(), Some("cli:tty0"));
    }

    #[test]
    fn decision_display() {
        assert_eq!(format!("{}", Decision::Approve), "approved");
        assert_eq!(
            format!(
                "{}",
                Decision::Reject {
                    reason: "missing tests".into()
                }
            ),
            "rejected: missing tests"
        );
        assert_eq!(format!("{}", Decision::Discuss), "discuss");
        assert_eq!(format!("{}", Decision::SkipForNow), "skipped");
    }

    #[test]
    fn notification_creation() {
        let goal_id = Uuid::new_v4();
        let notif = Notification::info("Sub-goal 2 of 5 started").with_goal_id(goal_id);
        assert_eq!(notif.level, NotificationLevel::Info);
        assert_eq!(notif.goal_id, Some(goal_id));
    }

    #[test]
    fn interaction_request_serialization_round_trip() {
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Test", 2)
            .with_metadata("thread_id", "T123");
        let json = serde_json::to_string(&req).unwrap();
        let restored: InteractionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.interaction_id, req.interaction_id);
        assert_eq!(restored.kind, InteractionKind::DraftReview);
        assert_eq!(restored.metadata.get("thread_id").unwrap(), "T123");
    }

    #[test]
    fn interaction_response_serialization_round_trip() {
        let resp = InteractionResponse::new(
            Uuid::new_v4(),
            Decision::Reject {
                reason: "needs refactor".into(),
            },
        )
        .with_reasoning("too complex");
        let json = serde_json::to_string(&resp).unwrap();
        let restored: InteractionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.decision, resp.decision);
        assert_eq!(restored.reasoning.as_deref(), Some("too complex"));
    }

    #[test]
    fn notification_serialization_round_trip() {
        let notif = Notification::warning("Drift detected");
        let json = serde_json::to_string(&notif).unwrap();
        let restored: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.message, "Drift detected");
        assert_eq!(restored.level, NotificationLevel::Warning);
    }

    #[test]
    fn channel_capabilities_defaults() {
        let caps = ChannelCapabilities::default();
        assert!(!caps.supports_async);
        assert!(!caps.supports_rich_media);
        assert!(!caps.supports_threads);
    }

    #[test]
    fn interaction_kind_custom() {
        let kind = InteractionKind::Custom("webhook_alert".into());
        assert_eq!(format!("{}", kind), "custom:webhook_alert");

        let json = serde_json::to_string(&kind).unwrap();
        let restored: InteractionKind = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, kind);
    }

    #[test]
    fn interaction_request_display() {
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Test", 1);
        let display = format!("{}", req);
        assert!(display.contains("draft_review"));
        assert!(display.contains("blocking"));
    }
}
