// channel.rs — Channel delivery trait for routing questions and notifications to
// external interfaces.
//
// When an agent calls ta_ask_human, the question can be delivered to one or
// more external channels (Slack, Discord, email, etc.) in addition to or
// instead of the local `ta shell` interface.  Channels additionally support
// `deliver_notification` for event-driven notifications (goal_failed, etc.)
// driven by the Notification Rules Engine (notification.rs).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::notification::NotificationSeverity;

/// A question to be delivered through an external channel.
///
/// This is a simplified view of PendingQuestion, containing only the fields
/// that channel adapters need for rendering and delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelQuestion {
    /// Unique interaction ID — used to correlate responses.
    pub interaction_id: Uuid,
    /// Goal this question belongs to.
    pub goal_id: Uuid,
    /// The question text.
    pub question: String,
    /// Optional context about what the agent was doing.
    pub context: Option<String>,
    /// Expected response shape: "freeform", "yes_no", "choice".
    pub response_hint: String,
    /// Suggested choices when response_hint is "choice".
    pub choices: Vec<String>,
    /// Turn number in the conversation (1-based).
    pub turn: u32,
    /// The daemon's base URL for posting responses.
    /// Channels should POST to `{callback_url}/api/interactions/{interaction_id}/respond`.
    pub callback_url: String,
}

/// Result of a channel delivery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    /// Channel name that handled the delivery.
    pub channel: String,
    /// Channel-specific delivery ID (e.g., Slack message timestamp, Discord message ID).
    pub delivery_id: String,
    /// Whether the delivery was successful.
    pub success: bool,
    /// Error message if delivery failed.
    pub error: Option<String>,
}

/// An event-driven notification to be delivered through a channel.
///
/// Unlike `ChannelQuestion` (which requires a response), notifications are
/// one-way: the channel renders the message and delivers it without expecting
/// a reply.  This is used by the Notification Rules Engine to push lifecycle
/// events (goal failures, policy violations, etc.) to channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelNotification {
    /// Source event ID.
    pub event_id: String,
    /// The event type string (e.g., `"goal_failed"`).
    pub event_type: String,
    /// Rendered notification title.
    pub title: String,
    /// Rendered notification body (plain text).
    pub body: String,
    /// Severity level derived from the event type.
    pub severity: NotificationSeverity,
    /// Goal involved, if any.
    pub goal_id: Option<Uuid>,
    /// Additional key/value metadata from the event payload.
    pub metadata: HashMap<String, String>,
}

impl ChannelNotification {
    /// Severity as a lowercase string (`"info"`, `"warning"`, `"error"`, `"critical"`).
    pub fn severity_str(&self) -> &'static str {
        self.severity.as_str()
    }
}

/// Trait for channel adapters that deliver questions and notifications to
/// external interfaces.
///
/// Each channel renders messages in its native format (Slack Block Kit,
/// Discord embeds, email HTML).
///
/// - `deliver_question`: interactive — requires a human response via the
///   daemon's `POST /api/interactions/:id/respond` endpoint.
/// - `deliver_notification`: one-way event push; default impl returns a
///   `success: false` result so unimplemented channels fail gracefully.
#[async_trait::async_trait]
pub trait ChannelDelivery: Send + Sync {
    /// Human-readable name of the channel (e.g., "slack", "discord", "email").
    fn name(&self) -> &str;

    /// Deliver an interactive question to the channel.
    ///
    /// Returns a `DeliveryResult` with the channel-specific delivery ID.
    async fn deliver_question(&self, question: &ChannelQuestion) -> DeliveryResult;

    /// Deliver a one-way event notification to the channel.
    ///
    /// The default implementation returns a failure result.  Channels that
    /// support notifications should override this method.
    async fn deliver_notification(&self, _notification: &ChannelNotification) -> DeliveryResult {
        DeliveryResult {
            channel: self.name().to_string(),
            delivery_id: String::new(),
            success: false,
            error: Some(format!(
                "Channel '{}' does not implement deliver_notification(). \
                 Override the method in the channel adapter to enable event notifications.",
                self.name()
            )),
        }
    }

    /// Validate that the channel's configuration is correct and reachable.
    async fn validate(&self) -> Result<(), String>;
}

/// Channel routing configuration — which channels to deliver to for a given event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelRouting {
    /// Explicit list of channel names to deliver to.
    /// Empty means use the daemon's default channel list.
    #[serde(default)]
    pub channels: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_question_serialization() {
        let q = ChannelQuestion {
            interaction_id: Uuid::new_v4(),
            goal_id: Uuid::new_v4(),
            question: "Which database should I use?".into(),
            context: Some("Setting up the backend".into()),
            response_hint: "choice".into(),
            choices: vec!["PostgreSQL".into(), "SQLite".into()],
            turn: 1,
            callback_url: "http://localhost:7700".into(),
        };
        let json = serde_json::to_string(&q).unwrap();
        let restored: ChannelQuestion = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.question, q.question);
        assert_eq!(restored.choices.len(), 2);
    }

    #[test]
    fn delivery_result_success() {
        let r = DeliveryResult {
            channel: "slack".into(),
            delivery_id: "1234567890.123456".into(),
            success: true,
            error: None,
        };
        assert!(r.success);
        assert!(r.error.is_none());
    }

    #[test]
    fn delivery_result_failure() {
        let r = DeliveryResult {
            channel: "discord".into(),
            delivery_id: String::new(),
            success: false,
            error: Some("Bot token invalid".into()),
        };
        assert!(!r.success);
        assert!(r.error.is_some());
    }

    #[test]
    fn channel_routing_default_is_empty() {
        let routing = ChannelRouting::default();
        assert!(routing.channels.is_empty());
    }

    #[test]
    fn channel_routing_serialization() {
        let routing = ChannelRouting {
            channels: vec!["slack".into(), "email".into()],
        };
        let json = serde_json::to_string(&routing).unwrap();
        let restored: ChannelRouting = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.channels.len(), 2);
    }

    #[test]
    fn channel_notification_serialization() {
        use crate::notification::NotificationSeverity;

        let n = ChannelNotification {
            event_id: "evt-001".into(),
            event_type: "goal_failed".into(),
            title: "[TA] Goal failed".into(),
            body: "Goal 'Fix bug' failed with exit code 1.".into(),
            severity: NotificationSeverity::Error,
            goal_id: Some(Uuid::new_v4()),
            metadata: {
                let mut m = HashMap::new();
                m.insert("title".into(), "Fix bug".into());
                m
            },
        };
        let json = serde_json::to_string(&n).unwrap();
        let restored: ChannelNotification = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.event_type, "goal_failed");
        assert_eq!(restored.severity, NotificationSeverity::Error);
        assert_eq!(restored.severity_str(), "error");
        assert!(restored.goal_id.is_some());
    }

    #[test]
    fn channel_notification_no_goal_id() {
        use crate::notification::NotificationSeverity;

        let n = ChannelNotification {
            event_id: "evt-002".into(),
            event_type: "policy_violation".into(),
            title: "[TA] Policy violation".into(),
            body: "A policy violation was detected.".into(),
            severity: NotificationSeverity::Critical,
            goal_id: None,
            metadata: HashMap::new(),
        };
        assert!(n.goal_id.is_none());
        assert_eq!(n.severity_str(), "critical");
    }
}
