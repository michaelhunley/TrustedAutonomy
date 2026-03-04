// context.rs — PolicyContext: runtime context for policy evaluation.
//
// The PolicyContext carries runtime state that policy rules can use for
// decisions: how much budget has been spent, how many actions the agent
// has taken, and the current drift score.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Runtime context passed alongside every policy evaluation request.
///
/// This allows policy rules to make decisions based on the current state
/// of the session, not just static grants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    /// The goal being worked on (if any).
    pub goal_id: Option<Uuid>,
    /// The session ID (if in a session).
    pub session_id: Option<Uuid>,
    /// The agent making the request.
    pub agent_id: String,
    /// Tokens spent so far in this goal.
    pub budget_spent: u64,
    /// Actions taken so far in this session.
    pub action_count: u32,
    /// Current behavioral drift score (0.0 = no drift, 1.0 = max drift).
    pub drift_score: Option<f64>,
}

impl PolicyContext {
    /// Create a minimal context with just an agent ID.
    pub fn new(agent_id: &str) -> Self {
        Self {
            goal_id: None,
            session_id: None,
            agent_id: agent_id.to_string(),
            budget_spent: 0,
            action_count: 0,
            drift_score: None,
        }
    }

    /// Set the goal ID.
    pub fn with_goal(mut self, goal_id: Uuid) -> Self {
        self.goal_id = Some(goal_id);
        self
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Check if the budget limit has been exceeded.
    pub fn is_over_budget(&self, limit: Option<u64>) -> bool {
        match limit {
            Some(max) => self.budget_spent >= max,
            None => false,
        }
    }

    /// Check if the budget warning threshold has been hit.
    pub fn is_budget_warning(&self, limit: Option<u64>, warn_percent: u8) -> bool {
        match limit {
            Some(max) if max > 0 => {
                let threshold = (max as f64 * warn_percent as f64 / 100.0) as u64;
                self.budget_spent >= threshold
            }
            _ => false,
        }
    }

    /// Check if drift score exceeds a threshold.
    pub fn is_drifting(&self, threshold: Option<f64>) -> bool {
        match (self.drift_score, threshold) {
            (Some(score), Some(thresh)) => score >= thresh,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_context_has_defaults() {
        let ctx = PolicyContext::new("agent-1");
        assert_eq!(ctx.agent_id, "agent-1");
        assert!(ctx.goal_id.is_none());
        assert_eq!(ctx.budget_spent, 0);
        assert_eq!(ctx.action_count, 0);
    }

    #[test]
    fn budget_check() {
        let mut ctx = PolicyContext::new("agent");
        ctx.budget_spent = 450_000;

        assert!(!ctx.is_over_budget(Some(500_000)));
        assert!(ctx.is_over_budget(Some(400_000)));
        assert!(!ctx.is_over_budget(None));
    }

    #[test]
    fn budget_warning() {
        let mut ctx = PolicyContext::new("agent");
        ctx.budget_spent = 420_000;

        // 80% of 500,000 = 400,000 → 420k exceeds warning
        assert!(ctx.is_budget_warning(Some(500_000), 80));
        // 90% of 500,000 = 450,000 → 420k doesn't exceed
        assert!(!ctx.is_budget_warning(Some(500_000), 90));
        // No limit → no warning
        assert!(!ctx.is_budget_warning(None, 80));
    }

    #[test]
    fn drift_check() {
        let mut ctx = PolicyContext::new("agent");

        // No drift score → not drifting
        assert!(!ctx.is_drifting(Some(0.5)));

        ctx.drift_score = Some(0.7);
        assert!(ctx.is_drifting(Some(0.5)));
        assert!(!ctx.is_drifting(Some(0.8)));
        assert!(!ctx.is_drifting(None));
    }

    #[test]
    fn builder_pattern() {
        let goal_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let ctx = PolicyContext::new("agent")
            .with_goal(goal_id)
            .with_session(session_id);

        assert_eq!(ctx.goal_id, Some(goal_id));
        assert_eq!(ctx.session_id, Some(session_id));
    }

    #[test]
    fn serialization_round_trip() {
        let ctx = PolicyContext::new("agent").with_goal(Uuid::new_v4());
        let json = serde_json::to_string(&ctx).unwrap();
        let restored: PolicyContext = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.agent_id, ctx.agent_id);
    }
}
