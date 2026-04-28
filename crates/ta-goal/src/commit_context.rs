// commit_context.rs — Minimal context passed to SourceAdapter operations.
//
// `CommitContext` contains the fields from `GoalRun` that the SourceAdapter
// methods actually use (prepare, commit, push, open_review). Extracting this
// struct breaks the `ta-submit` → `ta-goal` import cycle so that `ta-workspace`
// and `ta-goal` can import `ta-submit` without creating a circular dependency.
//
// Introduced in v0.15.29.1.

use std::path::PathBuf;
use uuid::Uuid;

/// Minimal context passed to SourceAdapter operations.
///
/// Extracted from GoalRun to break the ta-submit → ta-goal dependency cycle.
/// Adapters receive CommitContext instead of &GoalRun so that ta-workspace and
/// ta-goal can import ta-submit without creating a cycle.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommitContext {
    /// Unique identifier for this goal run.
    pub goal_run_id: Uuid,
    /// Human-readable title (e.g., "Fix authentication bug").
    pub title: String,
    /// Detailed objective describing what needs to be accomplished.
    pub objective: String,
    /// The agent working on this goal.
    pub agent_id: String,
    /// Optional plan phase this goal is working on (e.g., "4b").
    pub plan_phase: Option<String>,
    /// Path to the staging workspace directory.
    pub workspace_path: PathBuf,
    /// Path to the change store directory.
    pub store_path: PathBuf,
    /// Path to the original source project (for overlay-based goals).
    pub source_dir: Option<PathBuf>,
    /// PR URL created by `ta draft apply`.
    pub pr_url: Option<String>,
    /// Human-friendly tag for this goal.
    pub tag: Option<String>,
}

impl CommitContext {
    /// Short 8-char prefix of the goal UUID, used in branch names and display.
    pub fn shortref(&self) -> String {
        self.goal_run_id.to_string()[..8].to_string()
    }
}

impl From<&crate::goal_run::GoalRun> for CommitContext {
    fn from(g: &crate::goal_run::GoalRun) -> Self {
        CommitContext {
            goal_run_id: g.goal_run_id,
            title: g.title.clone(),
            objective: g.objective.clone(),
            agent_id: g.agent_id.clone(),
            plan_phase: g.plan_phase.clone(),
            workspace_path: g.workspace_path.clone(),
            store_path: g.store_path.clone(),
            source_dir: g.source_dir.clone(),
            pr_url: g.pr_url.clone(),
            tag: g.tag.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal_run::GoalRun;

    #[test]
    fn commit_context_shortref_is_8_chars() {
        let dir = tempfile::tempdir().unwrap();
        let goal = GoalRun::new(
            "Test goal",
            "Test objective",
            "test-agent",
            dir.path().to_path_buf(),
            dir.path().join("store"),
        );
        let ctx = CommitContext::from(&goal);
        assert_eq!(ctx.shortref().len(), 8);
        assert_eq!(ctx.shortref(), goal.goal_run_id.to_string()[..8]);
    }

    #[test]
    fn commit_context_from_goal_run_copies_fields() {
        let dir = tempfile::tempdir().unwrap();
        let mut goal = GoalRun::new(
            "My title",
            "My objective",
            "agent-1",
            dir.path().to_path_buf(),
            dir.path().join("store"),
        );
        goal.plan_phase = Some("v0.15.29.1".to_string());
        goal.tag = Some("my-tag".to_string());

        let ctx = CommitContext::from(&goal);
        assert_eq!(ctx.goal_run_id, goal.goal_run_id);
        assert_eq!(ctx.title, "My title");
        assert_eq!(ctx.objective, "My objective");
        assert_eq!(ctx.agent_id, "agent-1");
        assert_eq!(ctx.plan_phase, Some("v0.15.29.1".to_string()));
        assert_eq!(ctx.tag, Some("my-tag".to_string()));
    }
}
