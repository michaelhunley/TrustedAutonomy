// session_channel.rs — Session interaction protocol for human-agent communication.
//
// A `SessionChannel` trait that any frontend implements — CLI, Discord, Slack,
// email, or web app. The protocol is the same: TA doesn't care where the message
// came from, only that it's authenticated and routed to the right session.
//
// This is the core abstraction for Phase v0.3.1.2 (Interactive Session Orchestration).

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A bidirectional channel between a human and a TA-mediated agent session.
///
/// Every interaction between human and TA is a message on a channel.
/// The CLI is one channel. A Discord thread is another. The protocol is the same.
pub trait SessionChannel: Send + Sync {
    /// Display agent output to the human (streaming).
    fn emit(&self, event: &SessionEvent) -> Result<(), SessionChannelError>;

    /// Receive human input (blocks until available or timeout).
    /// Returns `None` on timeout.
    fn receive(&self, timeout: Duration) -> Result<Option<HumanInput>, SessionChannelError>;

    /// Channel identity (for audit trail).
    /// e.g., "cli:tty0", "discord:thread:123", "slack:C04:1234567890"
    fn channel_id(&self) -> &str;
}

/// Events emitted from TA/agent to the human.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum SessionEvent {
    /// Agent output (stdout or stderr).
    AgentOutput {
        stream: OutputStream,
        content: String,
    },

    /// A draft is ready for review (checkpoint).
    DraftReady {
        draft_id: Uuid,
        summary: String,
        artifact_count: usize,
    },

    /// The goal is complete.
    GoalComplete { goal_id: Uuid },

    /// Agent is waiting for human guidance.
    WaitingForInput { prompt: String },

    /// Session status update (informational).
    StatusUpdate { message: String },
}

impl fmt::Display for SessionEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionEvent::AgentOutput { stream, content } => {
                write!(f, "[{}] {}", stream, content)
            }
            SessionEvent::DraftReady {
                draft_id,
                summary,
                artifact_count,
            } => {
                write!(
                    f,
                    "Draft ready: {} ({} artifacts) — {}",
                    draft_id, artifact_count, summary
                )
            }
            SessionEvent::GoalComplete { goal_id } => {
                write!(f, "Goal complete: {}", goal_id)
            }
            SessionEvent::WaitingForInput { prompt } => {
                write!(f, "Waiting for input: {}", prompt)
            }
            SessionEvent::StatusUpdate { message } => {
                write!(f, "Status: {}", message)
            }
        }
    }
}

/// Output stream identifier (stdout vs stderr).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    StdOut,
    StdErr,
}

impl fmt::Display for OutputStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputStream::StdOut => write!(f, "stdout"),
            OutputStream::StdErr => write!(f, "stderr"),
        }
    }
}

/// Input from a human to the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "input_type", rename_all = "snake_case")]
pub enum HumanInput {
    /// Free-form guidance message injected into agent context.
    Message { text: String },

    /// Inline review: approve a draft artifact.
    Approve {
        draft_id: Uuid,
        artifact_uri: Option<String>,
    },

    /// Inline review: reject a draft artifact with reason.
    Reject {
        draft_id: Uuid,
        artifact_uri: Option<String>,
        reason: String,
    },

    /// Abort the session.
    Abort,
}

impl fmt::Display for HumanInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HumanInput::Message { text } => write!(f, "Message: {}", text),
            HumanInput::Approve {
                draft_id,
                artifact_uri,
            } => {
                if let Some(uri) = artifact_uri {
                    write!(f, "Approve {} in draft {}", uri, draft_id)
                } else {
                    write!(f, "Approve draft {}", draft_id)
                }
            }
            HumanInput::Reject {
                draft_id,
                artifact_uri,
                reason,
            } => {
                if let Some(uri) = artifact_uri {
                    write!(f, "Reject {} in draft {}: {}", uri, draft_id, reason)
                } else {
                    write!(f, "Reject draft {}: {}", draft_id, reason)
                }
            }
            HumanInput::Abort => write!(f, "Abort session"),
        }
    }
}

/// Errors from session channel operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionChannelError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("channel closed")]
    ChannelClosed,

    #[error("session error: {0}")]
    Other(String),
}

/// A persistent interactive session record linking a goal to channel state.
///
/// Tracks the lifecycle of a human-agent interactive session across CLI invocations.
/// Serialized to JSON for persistence (same pattern as ReviewSession).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveSession {
    /// Unique session identifier.
    pub session_id: Uuid,

    /// The GoalRun this session is attached to.
    pub goal_id: Uuid,

    /// Channel identity (e.g., "cli:tty0").
    pub channel_id: String,

    /// Agent identity (e.g., "claude-code").
    pub agent_id: String,

    /// Session lifecycle state.
    pub state: InteractiveSessionState,

    /// Session creation time.
    pub created_at: DateTime<Utc>,

    /// Last activity time.
    pub updated_at: DateTime<Utc>,

    /// Message log (persisted for audit and resume).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<SessionMessage>,

    /// Associated draft IDs (drafts reviewed inline during this session).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draft_ids: Vec<Uuid>,
}

/// Session lifecycle states.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractiveSessionState {
    /// Session is active (agent running, human connected).
    Active,
    /// Session is paused (agent suspended, can be resumed).
    Paused,
    /// Session completed successfully.
    Completed,
    /// Session was aborted by the human.
    Aborted,
}

impl fmt::Display for InteractiveSessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InteractiveSessionState::Active => write!(f, "active"),
            InteractiveSessionState::Paused => write!(f, "paused"),
            InteractiveSessionState::Completed => write!(f, "completed"),
            InteractiveSessionState::Aborted => write!(f, "aborted"),
        }
    }
}

/// A logged message in a session (for audit and replay).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    /// Who sent the message ("human", "agent", "ta-system").
    pub sender: String,
    /// Message content.
    pub content: String,
    /// When the message was sent.
    pub timestamp: DateTime<Utc>,
}

impl InteractiveSession {
    /// Create a new interactive session for a goal.
    pub fn new(goal_id: Uuid, channel_id: String, agent_id: String) -> Self {
        let now = Utc::now();
        Self {
            session_id: Uuid::new_v4(),
            goal_id,
            channel_id,
            agent_id,
            state: InteractiveSessionState::Active,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            draft_ids: Vec::new(),
        }
    }

    /// Record a message in the session log.
    pub fn log_message(&mut self, sender: &str, content: &str) {
        self.messages.push(SessionMessage {
            sender: sender.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
        });
        self.updated_at = Utc::now();
    }

    /// Record that a draft was reviewed inline during this session.
    pub fn add_draft(&mut self, draft_id: Uuid) {
        if !self.draft_ids.contains(&draft_id) {
            self.draft_ids.push(draft_id);
        }
        self.updated_at = Utc::now();
    }

    /// Transition to a new state.
    pub fn transition(
        &mut self,
        new_state: InteractiveSessionState,
    ) -> Result<(), SessionChannelError> {
        let valid = matches!(
            (&self.state, &new_state),
            (
                InteractiveSessionState::Active,
                InteractiveSessionState::Paused
            ) | (
                InteractiveSessionState::Active,
                InteractiveSessionState::Completed
            ) | (
                InteractiveSessionState::Active,
                InteractiveSessionState::Aborted
            ) | (
                InteractiveSessionState::Paused,
                InteractiveSessionState::Active
            ) | (
                InteractiveSessionState::Paused,
                InteractiveSessionState::Aborted
            )
        );

        if !valid {
            return Err(SessionChannelError::Other(format!(
                "invalid session transition from {} to {}",
                self.state, new_state
            )));
        }

        self.state = new_state;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Check if the session is in an active or paused state (can still be used).
    pub fn is_alive(&self) -> bool {
        matches!(
            self.state,
            InteractiveSessionState::Active | InteractiveSessionState::Paused
        )
    }

    /// Get elapsed time since session start.
    pub fn elapsed(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }

    /// Format elapsed time as a human-readable string.
    pub fn elapsed_display(&self) -> String {
        let dur = self.elapsed();
        let secs = dur.num_seconds();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }
}

/// Per-agent interactive configuration (loaded from YAML).
///
/// Extends the AgentLaunchConfig with interactive session settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InteractiveConfig {
    /// Whether interactive mode is available for this agent.
    #[serde(default)]
    pub enabled: bool,

    /// Output capture mode: "pty", "pipe", or "log".
    #[serde(default = "default_output_capture")]
    pub output_capture: String,

    /// Whether to allow human input injection during agent execution.
    #[serde(default = "default_true")]
    pub allow_human_input: bool,

    /// Auto-exit condition: "idle_timeout: 300s" or "goal_complete".
    #[serde(default)]
    pub auto_exit_on: Option<String>,

    /// Override launch command for resume (e.g., "claude --resume {session_id}").
    #[serde(default)]
    pub resume_cmd: Option<String>,
}

fn default_output_capture() -> String {
    "pipe".to_string()
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_interactive_session_is_active() {
        let session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );
        assert_eq!(session.state, InteractiveSessionState::Active);
        assert!(session.messages.is_empty());
        assert!(session.draft_ids.is_empty());
        assert!(session.is_alive());
    }

    #[test]
    fn log_message_adds_to_history() {
        let mut session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );

        session.log_message("human", "Focus on the auth module");
        session.log_message("agent", "Understood, working on auth");

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].sender, "human");
        assert_eq!(session.messages[1].sender, "agent");
    }

    #[test]
    fn add_draft_deduplicates() {
        let mut session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );
        let draft_id = Uuid::new_v4();

        session.add_draft(draft_id);
        session.add_draft(draft_id);

        assert_eq!(session.draft_ids.len(), 1);
    }

    #[test]
    fn valid_transitions() {
        let mut session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );

        // Active → Paused
        session.transition(InteractiveSessionState::Paused).unwrap();
        assert_eq!(session.state, InteractiveSessionState::Paused);

        // Paused → Active
        session.transition(InteractiveSessionState::Active).unwrap();
        assert_eq!(session.state, InteractiveSessionState::Active);

        // Active → Completed
        session
            .transition(InteractiveSessionState::Completed)
            .unwrap();
        assert_eq!(session.state, InteractiveSessionState::Completed);
    }

    #[test]
    fn invalid_transition_returns_error() {
        let mut session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );
        session
            .transition(InteractiveSessionState::Completed)
            .unwrap();

        // Completed → Active should fail
        let result = session.transition(InteractiveSessionState::Active);
        assert!(result.is_err());
    }

    #[test]
    fn abort_from_active() {
        let mut session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );
        session
            .transition(InteractiveSessionState::Aborted)
            .unwrap();
        assert!(!session.is_alive());
    }

    #[test]
    fn abort_from_paused() {
        let mut session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );
        session.transition(InteractiveSessionState::Paused).unwrap();
        session
            .transition(InteractiveSessionState::Aborted)
            .unwrap();
        assert!(!session.is_alive());
    }

    #[test]
    fn session_serialization_round_trip() {
        let mut session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );
        session.log_message("human", "Test message");
        session.add_draft(Uuid::new_v4());

        let json = serde_json::to_string(&session).unwrap();
        let restored: InteractiveSession = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.session_id, session.session_id);
        assert_eq!(restored.goal_id, session.goal_id);
        assert_eq!(restored.channel_id, session.channel_id);
        assert_eq!(restored.agent_id, session.agent_id);
        assert_eq!(restored.messages.len(), 1);
        assert_eq!(restored.draft_ids.len(), 1);
    }

    #[test]
    fn session_event_display() {
        let event = SessionEvent::AgentOutput {
            stream: OutputStream::StdOut,
            content: "Hello world".to_string(),
        };
        assert_eq!(format!("{}", event), "[stdout] Hello world");

        let event = SessionEvent::WaitingForInput {
            prompt: "What next?".to_string(),
        };
        assert_eq!(format!("{}", event), "Waiting for input: What next?");
    }

    #[test]
    fn human_input_display() {
        let input = HumanInput::Message {
            text: "Focus on auth".to_string(),
        };
        assert_eq!(format!("{}", input), "Message: Focus on auth");

        let input = HumanInput::Abort;
        assert_eq!(format!("{}", input), "Abort session");
    }

    #[test]
    fn output_stream_display() {
        assert_eq!(format!("{}", OutputStream::StdOut), "stdout");
        assert_eq!(format!("{}", OutputStream::StdErr), "stderr");
    }

    #[test]
    fn interactive_config_defaults() {
        let config: InteractiveConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.enabled);
        assert_eq!(config.output_capture, "pipe");
        assert!(config.allow_human_input);
        assert!(config.auto_exit_on.is_none());
        assert!(config.resume_cmd.is_none());
    }

    #[test]
    fn interactive_config_from_yaml() {
        let yaml = r#"
enabled: true
output_capture: pty
allow_human_input: true
auto_exit_on: "idle_timeout: 300s"
resume_cmd: "claude --resume {session_id}"
"#;
        let config: InteractiveConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.output_capture, "pty");
        assert!(config.allow_human_input);
        assert_eq!(config.auto_exit_on.as_deref(), Some("idle_timeout: 300s"));
        assert_eq!(
            config.resume_cmd.as_deref(),
            Some("claude --resume {session_id}")
        );
    }

    #[test]
    fn elapsed_display_formatting() {
        let mut session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );
        // Just created — should show a very small duration.
        let display = session.elapsed_display();
        assert!(display.ends_with('s'));

        // Manually set created_at to the past for testing.
        session.created_at = Utc::now() - chrono::Duration::minutes(5);
        let display = session.elapsed_display();
        assert!(display.contains('m'));
    }

    #[test]
    fn session_event_serialization_round_trip() {
        let event = SessionEvent::DraftReady {
            draft_id: Uuid::new_v4(),
            summary: "Test draft".to_string(),
            artifact_count: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: SessionEvent = serde_json::from_str(&json).unwrap();
        if let SessionEvent::DraftReady { artifact_count, .. } = restored {
            assert_eq!(artifact_count, 3);
        } else {
            panic!("Expected DraftReady variant");
        }
    }

    #[test]
    fn human_input_serialization_round_trip() {
        let input = HumanInput::Reject {
            draft_id: Uuid::new_v4(),
            artifact_uri: Some("fs://workspace/main.rs".to_string()),
            reason: "needs error handling".to_string(),
        };
        let json = serde_json::to_string(&input).unwrap();
        let restored: HumanInput = serde_json::from_str(&json).unwrap();
        if let HumanInput::Reject { reason, .. } = restored {
            assert_eq!(reason, "needs error handling");
        } else {
            panic!("Expected Reject variant");
        }
    }
}
