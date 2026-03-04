// session.rs — TaSession: the core session object (stub for workspace build).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::SessionError;

/// The core session object — one session per goal.
///
/// Tracks the full lifecycle from agent launch through review iterations
/// to final approval. Provides conversational continuity: when the human
/// rejects a draft with feedback, TA relaunches the agent with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaSession {
    pub session_id: Uuid,
    pub goal_id: Uuid,
    pub agent_id: String,
    pub state: SessionState,
    pub conversation: Vec<ConversationTurn>,
    pub pending_draft: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub iteration_count: u32,
    pub checkpoint_mode: bool,
}

/// Session lifecycle states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session created, agent not yet launched.
    Starting,
    /// Agent is executing in the staging workspace.
    AgentRunning,
    /// Agent exited, draft has been built.
    DraftReady,
    /// Draft submitted, waiting for human review.
    WaitingForReview,
    /// Human rejected, preparing to relaunch with feedback.
    Iterating,
    /// Draft approved and applied — session complete.
    Completed,
    /// Human aborted the session.
    Aborted,
    /// Session paused by human.
    Paused,
    /// An error occurred.
    Failed { reason: String },
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Starting => write!(f, "starting"),
            SessionState::AgentRunning => write!(f, "agent_running"),
            SessionState::DraftReady => write!(f, "draft_ready"),
            SessionState::WaitingForReview => write!(f, "waiting_for_review"),
            SessionState::Iterating => write!(f, "iterating"),
            SessionState::Completed => write!(f, "completed"),
            SessionState::Aborted => write!(f, "aborted"),
            SessionState::Paused => write!(f, "paused"),
            SessionState::Failed { reason } => write!(f, "failed: {}", reason),
        }
    }
}

/// A single turn in the human↔agent conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub turn_id: Uuid,
    pub iteration: u32,
    pub agent_context: Option<String>,
    pub human_feedback: Option<String>,
    pub draft_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
}

impl TaSession {
    /// Create a new session for a goal.
    pub fn new(goal_id: Uuid, agent_id: &str) -> Self {
        let now = Utc::now();
        Self {
            session_id: Uuid::new_v4(),
            goal_id,
            agent_id: agent_id.to_string(),
            state: SessionState::Starting,
            conversation: Vec::new(),
            pending_draft: None,
            created_at: now,
            updated_at: now,
            iteration_count: 0,
            checkpoint_mode: false,
        }
    }

    /// Enable checkpoint mode (for batch/CI workflows).
    pub fn with_checkpoint_mode(mut self) -> Self {
        self.checkpoint_mode = true;
        self
    }

    /// Transition to a new state, validating the transition.
    pub fn transition(&mut self, new_state: SessionState) -> Result<(), SessionError> {
        let valid = matches!(
            (&self.state, &new_state),
            // Normal forward flow
            (SessionState::Starting, SessionState::AgentRunning)
                | (SessionState::AgentRunning, SessionState::DraftReady)
                | (SessionState::DraftReady, SessionState::WaitingForReview)
                | (SessionState::WaitingForReview, SessionState::Completed)
                | (SessionState::WaitingForReview, SessionState::Iterating)
                | (SessionState::Iterating, SessionState::AgentRunning)
                // Pause/resume
                | (SessionState::AgentRunning, SessionState::Paused)
                | (SessionState::Paused, SessionState::AgentRunning)
                // Abort from any active state
                | (SessionState::Starting, SessionState::Aborted)
                | (SessionState::AgentRunning, SessionState::Aborted)
                | (SessionState::DraftReady, SessionState::Aborted)
                | (SessionState::WaitingForReview, SessionState::Aborted)
                | (SessionState::Iterating, SessionState::Aborted)
                | (SessionState::Paused, SessionState::Aborted)
                // Fail from any active state
                | (SessionState::Starting, SessionState::Failed { .. })
                | (SessionState::AgentRunning, SessionState::Failed { .. })
                | (SessionState::DraftReady, SessionState::Failed { .. })
                | (SessionState::WaitingForReview, SessionState::Failed { .. })
                | (SessionState::Iterating, SessionState::Failed { .. })
                | (SessionState::Paused, SessionState::Failed { .. })
        );

        if !valid {
            return Err(SessionError::InvalidTransition {
                from: self.state.to_string(),
                to: new_state.to_string(),
            });
        }

        self.state = new_state;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Record a new conversation turn.
    pub fn add_turn(
        &mut self,
        agent_context: Option<String>,
        human_feedback: Option<String>,
        draft_id: Option<Uuid>,
    ) {
        self.iteration_count += 1;
        self.conversation.push(ConversationTurn {
            turn_id: Uuid::new_v4(),
            iteration: self.iteration_count,
            agent_context,
            human_feedback,
            draft_id,
            timestamp: Utc::now(),
        });
        self.updated_at = Utc::now();
    }

    /// Set the pending draft ID.
    pub fn set_pending_draft(&mut self, draft_id: Uuid) {
        self.pending_draft = Some(draft_id);
        self.updated_at = Utc::now();
    }

    /// Clear the pending draft.
    pub fn clear_pending_draft(&mut self) {
        self.pending_draft = None;
        self.updated_at = Utc::now();
    }

    /// Check if the session is in an active state (can still progress).
    pub fn is_active(&self) -> bool {
        !matches!(
            self.state,
            SessionState::Completed | SessionState::Aborted | SessionState::Failed { .. }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_starts_in_starting_state() {
        let session = TaSession::new(Uuid::new_v4(), "claude-code");
        assert_eq!(session.state, SessionState::Starting);
        assert!(session.is_active());
        assert_eq!(session.iteration_count, 0);
        assert!(session.conversation.is_empty());
    }

    #[test]
    fn normal_lifecycle_flow() {
        let mut session = TaSession::new(Uuid::new_v4(), "claude-code");

        session.transition(SessionState::AgentRunning).unwrap();
        assert_eq!(session.state, SessionState::AgentRunning);

        session.transition(SessionState::DraftReady).unwrap();
        assert_eq!(session.state, SessionState::DraftReady);

        session.transition(SessionState::WaitingForReview).unwrap();
        assert_eq!(session.state, SessionState::WaitingForReview);

        session.transition(SessionState::Completed).unwrap();
        assert_eq!(session.state, SessionState::Completed);
        assert!(!session.is_active());
    }

    #[test]
    fn iteration_loop() {
        let mut session = TaSession::new(Uuid::new_v4(), "claude-code");

        // First iteration
        session.transition(SessionState::AgentRunning).unwrap();
        session.transition(SessionState::DraftReady).unwrap();
        session.transition(SessionState::WaitingForReview).unwrap();

        // Human rejects → iterate
        session.transition(SessionState::Iterating).unwrap();
        session.transition(SessionState::AgentRunning).unwrap();

        // Second iteration
        session.transition(SessionState::DraftReady).unwrap();
        session.transition(SessionState::WaitingForReview).unwrap();
        session.transition(SessionState::Completed).unwrap();
    }

    #[test]
    fn pause_and_resume() {
        let mut session = TaSession::new(Uuid::new_v4(), "claude-code");
        session.transition(SessionState::AgentRunning).unwrap();

        session.transition(SessionState::Paused).unwrap();
        assert_eq!(session.state, SessionState::Paused);
        assert!(session.is_active());

        session.transition(SessionState::AgentRunning).unwrap();
        assert_eq!(session.state, SessionState::AgentRunning);
    }

    #[test]
    fn abort_from_active_states() {
        for initial in [
            SessionState::Starting,
            SessionState::AgentRunning,
            SessionState::DraftReady,
            SessionState::WaitingForReview,
            SessionState::Iterating,
            SessionState::Paused,
        ] {
            let mut session = TaSession::new(Uuid::new_v4(), "agent");
            session.state = initial;
            session.transition(SessionState::Aborted).unwrap();
            assert!(!session.is_active());
        }
    }

    #[test]
    fn invalid_transition_rejected() {
        let mut session = TaSession::new(Uuid::new_v4(), "agent");
        session.transition(SessionState::Completed).unwrap_err();

        let mut session2 = TaSession::new(Uuid::new_v4(), "agent");
        session2.state = SessionState::Completed;
        session2.transition(SessionState::AgentRunning).unwrap_err();
    }

    #[test]
    fn add_turn_increments_iteration() {
        let mut session = TaSession::new(Uuid::new_v4(), "agent");
        session.add_turn(Some("context".to_string()), None, None);
        assert_eq!(session.iteration_count, 1);
        assert_eq!(session.conversation.len(), 1);
        assert_eq!(session.conversation[0].iteration, 1);

        session.add_turn(None, Some("fix the bug".to_string()), None);
        assert_eq!(session.iteration_count, 2);
        assert_eq!(session.conversation[1].iteration, 2);
    }

    #[test]
    fn pending_draft_management() {
        let mut session = TaSession::new(Uuid::new_v4(), "agent");
        assert!(session.pending_draft.is_none());

        let draft_id = Uuid::new_v4();
        session.set_pending_draft(draft_id);
        assert_eq!(session.pending_draft, Some(draft_id));

        session.clear_pending_draft();
        assert!(session.pending_draft.is_none());
    }

    #[test]
    fn serialization_round_trip() {
        let mut session = TaSession::new(Uuid::new_v4(), "claude-code");
        session.add_turn(Some("initial context".to_string()), None, None);
        session.set_pending_draft(Uuid::new_v4());

        let json = serde_json::to_string(&session).unwrap();
        let restored: TaSession = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.session_id, session.session_id);
        assert_eq!(restored.goal_id, session.goal_id);
        assert_eq!(restored.agent_id, session.agent_id);
        assert_eq!(restored.iteration_count, 1);
        assert_eq!(restored.conversation.len(), 1);
    }

    #[test]
    fn checkpoint_mode() {
        let session = TaSession::new(Uuid::new_v4(), "agent").with_checkpoint_mode();
        assert!(session.checkpoint_mode);
    }

    #[test]
    fn session_state_display() {
        assert_eq!(format!("{}", SessionState::Starting), "starting");
        assert_eq!(format!("{}", SessionState::AgentRunning), "agent_running");
        assert_eq!(
            format!("{}", SessionState::WaitingForReview),
            "waiting_for_review"
        );
        assert_eq!(
            format!(
                "{}",
                SessionState::Failed {
                    reason: "oops".to_string()
                }
            ),
            "failed: oops"
        );
    }

    #[test]
    fn elapsed_display_formatting() {
        let session = TaSession::new(Uuid::new_v4(), "agent");
        let display = session.elapsed_display();
        assert!(display.ends_with('s'));
    }
}
