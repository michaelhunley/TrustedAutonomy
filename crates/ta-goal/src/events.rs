// events.rs — Event model and notification dispatch.
//
// TA emits events at key lifecycle points. Notification sinks (log files,
// webhook scripts, Discord, email) subscribe to these events.
//
// This aligns with the Plugin Architecture guidance:
// - Plugins observe and advise, they cannot bypass policy
// - Events use stable types that plugins can depend on
// - The dispatcher is synchronous for now; async dispatch is a future enhancement
//
// Core event hooks from the architecture:
//   Goal lifecycle: on_goal_created, on_goal_configured, on_goal_started, etc.
//   Execution: on_changeset_created, on_pr_generated, on_pr_approved, on_pr_denied
//   Audit: on_policy_violation, on_anomaly_detected

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::GoalError;
use crate::goal_run::GoalRunState;

/// Events emitted by TA at key lifecycle points.
///
/// These are the stable event types that notification sinks and future
/// plugins can subscribe to. The enum variants map to the event hooks
/// defined in the plugin architecture guidance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum TaEvent {
    /// A new GoalRun was created.
    GoalCreated {
        goal_run_id: Uuid,
        title: String,
        agent_id: String,
        timestamp: DateTime<Utc>,
    },

    /// A GoalRun changed state.
    GoalStateChanged {
        goal_run_id: Uuid,
        from_state: String,
        to_state: String,
        timestamp: DateTime<Utc>,
    },

    /// A PR package is ready for review.
    PrReady {
        goal_run_id: Uuid,
        pr_package_id: Uuid,
        summary: String,
        timestamp: DateTime<Utc>,
    },

    /// A PR package was approved.
    PrApproved {
        goal_run_id: Uuid,
        pr_package_id: Uuid,
        approved_by: String,
        timestamp: DateTime<Utc>,
    },

    /// A PR package was denied.
    PrDenied {
        goal_run_id: Uuid,
        pr_package_id: Uuid,
        reason: String,
        denied_by: String,
        timestamp: DateTime<Utc>,
    },

    /// Approved changes were applied to the target.
    ChangesApplied {
        goal_run_id: Uuid,
        files: Vec<String>,
        target_dir: String,
        timestamp: DateTime<Utc>,
    },

    /// A ChangeSet was created (file staged).
    ChangesetCreated {
        goal_run_id: Uuid,
        changeset_id: Uuid,
        target_uri: String,
        timestamp: DateTime<Utc>,
    },
}

impl TaEvent {
    /// Get the event type name as a string.
    pub fn event_type(&self) -> &str {
        match self {
            TaEvent::GoalCreated { .. } => "goal_created",
            TaEvent::GoalStateChanged { .. } => "goal_state_changed",
            TaEvent::PrReady { .. } => "pr_ready",
            TaEvent::PrApproved { .. } => "pr_approved",
            TaEvent::PrDenied { .. } => "pr_denied",
            TaEvent::ChangesApplied { .. } => "changes_applied",
            TaEvent::ChangesetCreated { .. } => "changeset_created",
        }
    }

    /// Helper to create a GoalCreated event.
    pub fn goal_created(goal_run_id: Uuid, title: &str, agent_id: &str) -> Self {
        TaEvent::GoalCreated {
            goal_run_id,
            title: title.to_string(),
            agent_id: agent_id.to_string(),
            timestamp: Utc::now(),
        }
    }

    /// Helper to create a GoalStateChanged event.
    pub fn goal_state_changed(goal_run_id: Uuid, from: &GoalRunState, to: &GoalRunState) -> Self {
        TaEvent::GoalStateChanged {
            goal_run_id,
            from_state: from.to_string(),
            to_state: to.to_string(),
            timestamp: Utc::now(),
        }
    }
}

/// Trait for receiving TA events.
///
/// Implementations decide what to do with each event: log to a file,
/// call a webhook, send a Discord message, etc.
///
/// This is the foundation of the plugin architecture. In Phase 3+,
/// this trait will be extended with filtering (subscribe to specific
/// event types) and async dispatch.
pub trait NotificationSink: Send {
    /// Handle an event. Errors are logged but don't stop the system.
    fn send(&self, event: &TaEvent) -> Result<(), GoalError>;
}

/// Logs events as JSONL to a file (always-on sink).
pub struct LogSink {
    path: PathBuf,
}

impl LogSink {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl NotificationSink for LogSink {
    fn send(&self, event: &TaEvent) -> Result<(), GoalError> {
        // Ensure parent directory exists.
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| GoalError::IoError {
                path: parent.display().to_string(),
                source,
            })?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| GoalError::IoError {
                path: self.path.display().to_string(),
                source,
            })?;

        let json = serde_json::to_string(event)?;
        writeln!(file, "{}", json).map_err(|source| GoalError::IoError {
            path: self.path.display().to_string(),
            source,
        })?;

        Ok(())
    }
}

/// Dispatches events to multiple sinks.
///
/// Errors from individual sinks are logged (via tracing) but don't
/// prevent other sinks from receiving the event.
pub struct EventDispatcher {
    sinks: Vec<Box<dyn NotificationSink>>,
}

impl EventDispatcher {
    /// Create a new dispatcher with no sinks.
    pub fn new() -> Self {
        Self { sinks: Vec::new() }
    }

    /// Add a notification sink.
    pub fn add_sink(&mut self, sink: Box<dyn NotificationSink>) {
        self.sinks.push(sink);
    }

    /// Dispatch an event to all sinks.
    pub fn dispatch(&self, event: &TaEvent) {
        for sink in &self.sinks {
            if let Err(e) = sink.send(event) {
                tracing::warn!("notification sink error: {}", e);
            }
        }
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn event_serialization_round_trip() {
        let event = TaEvent::goal_created(Uuid::new_v4(), "Test Goal", "agent-1");
        let json = serde_json::to_string(&event).unwrap();
        let restored: TaEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.event_type(), restored.event_type());
        assert!(json.contains("\"goal_created\""));
    }

    #[test]
    fn log_sink_appends_to_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let sink = LogSink::new(&path);

        let event1 = TaEvent::goal_created(Uuid::new_v4(), "Goal 1", "agent-1");
        let event2 = TaEvent::goal_created(Uuid::new_v4(), "Goal 2", "agent-2");

        sink.send(&event1).unwrap();
        sink.send(&event2).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn dispatcher_sends_to_all_sinks() {
        let dir = tempdir().unwrap();
        let path1 = dir.path().join("sink1.jsonl");
        let path2 = dir.path().join("sink2.jsonl");

        let mut dispatcher = EventDispatcher::new();
        dispatcher.add_sink(Box::new(LogSink::new(&path1)));
        dispatcher.add_sink(Box::new(LogSink::new(&path2)));

        let event = TaEvent::goal_created(Uuid::new_v4(), "Test", "agent-1");
        dispatcher.dispatch(&event);

        // Both sinks should have received the event.
        assert!(fs::read_to_string(&path1).unwrap().contains("goal_created"));
        assert!(fs::read_to_string(&path2).unwrap().contains("goal_created"));
    }

    #[test]
    fn event_type_names() {
        let id = Uuid::new_v4();
        assert_eq!(
            TaEvent::goal_created(id, "x", "y").event_type(),
            "goal_created"
        );
        assert_eq!(
            TaEvent::goal_state_changed(id, &GoalRunState::Created, &GoalRunState::Configured)
                .event_type(),
            "goal_state_changed"
        );
    }
}
