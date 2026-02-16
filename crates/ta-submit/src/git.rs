//! Git adapter for branch-based workflows with GitHub/GitLab PR creation

use std::process::Command;
use ta_changeset::DraftPackage;
use ta_goal::GoalRun;

use crate::adapter::{CommitResult, PushResult, Result, ReviewResult, SubmitAdapter, SubmitError};
use crate::config::SubmitConfig;

/// Git adapter implementing branch-based workflow
///
/// Features:
/// - Automatic feature branch creation
/// - Git commit with PR metadata
/// - Push to remote
/// - GitHub/GitLab PR creation via gh/glab CLI
pub struct GitAdapter {
    /// Working directory for git operations
    work_dir: std::path::PathBuf,
}

impl GitAdapter {
    /// Create a new GitAdapter for the given working directory
    pub fn new(work_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            work_dir: work_dir.into(),
        }
    }

    /// Run a git command in the working directory
    fn git_cmd(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.work_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SubmitError::VcsError(format!(
                "git {} failed: {}",
                args.join(" "),
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Check if gh CLI is available
    fn has_gh_cli(&self) -> bool {
        Command::new("gh")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Get current branch name
    pub fn current_branch(&self) -> Result<String> {
        self.git_cmd(&["rev-parse", "--abbrev-ref", "HEAD"])
    }

    /// Generate branch name from goal
    fn branch_name(&self, goal: &GoalRun, config: &SubmitConfig) -> String {
        let prefix = &config.git.branch_prefix;
        let sanitized = goal
            .title
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>();

        // Truncate to reasonable length
        let sanitized = if sanitized.len() > 50 {
            &sanitized[..50]
        } else {
            &sanitized
        };

        format!("{}{}", prefix, sanitized)
    }
}

impl SubmitAdapter for GitAdapter {
    fn prepare(&self, goal: &GoalRun, config: &SubmitConfig) -> Result<()> {
        let branch_name = self.branch_name(goal, config);

        tracing::info!("GitAdapter: creating branch {}", branch_name);

        // Check if branch already exists
        let branches = self.git_cmd(&["branch", "--list", &branch_name])?;
        if branches.is_empty() {
            // Create new branch
            self.git_cmd(&["checkout", "-b", &branch_name])?;
        } else {
            // Switch to existing branch
            self.git_cmd(&["checkout", &branch_name])?;
        }

        Ok(())
    }

    fn commit(&self, goal: &GoalRun, pr: &DraftPackage, message: &str) -> Result<CommitResult> {
        tracing::info!("GitAdapter: committing changes");

        // Add all changes (staging workspace is already filtered by .taignore)
        self.git_cmd(&["add", "."])?;

        // Check if there are changes to commit
        let status = self.git_cmd(&["status", "--porcelain"])?;
        if status.trim().is_empty() {
            return Err(SubmitError::InvalidState(
                "No changes to commit".to_string(),
            ));
        }

        // Append metadata trailers to the caller-provided message.
        let phase_line = goal
            .plan_phase
            .as_ref()
            .map(|p| format!("\nPhase: {}", p))
            .unwrap_or_default();
        let commit_msg = format!(
            "{}\n\nGoal-ID: {}\nPR-ID: {}{}\n\nCo-Authored-By: Trusted Autonomy <ta@trustedautonomy.dev>",
            message, goal.goal_run_id, pr.package_id, phase_line
        );

        // Commit
        self.git_cmd(&["commit", "-m", &commit_msg])?;

        // Get commit hash
        let commit_id = self.git_cmd(&["rev-parse", "HEAD"])?;

        Ok(CommitResult {
            commit_id: commit_id.clone(),
            message: format!("Committed as {}", &commit_id[..8]),
            metadata: [("full_hash".to_string(), commit_id)].into_iter().collect(),
        })
    }

    fn push(&self, goal: &GoalRun) -> Result<PushResult> {
        let config = SubmitConfig::default(); // TODO: pass config through
        let branch_name = self.branch_name(goal, &config);
        let remote = &config.git.remote;

        tracing::info!("GitAdapter: pushing branch {} to {}", branch_name, remote);

        // Push with --set-upstream
        self.git_cmd(&["push", "-u", remote, &branch_name])?;

        Ok(PushResult {
            remote_ref: format!("{}/{}", remote, branch_name),
            message: format!("Pushed to {}/{}", remote, branch_name),
            metadata: [
                ("branch".to_string(), branch_name),
                ("remote".to_string(), remote.clone()),
            ]
            .into_iter()
            .collect(),
        })
    }

    fn open_review(&self, goal: &GoalRun, pr: &DraftPackage) -> Result<ReviewResult> {
        if !self.has_gh_cli() {
            return Err(SubmitError::ReviewError(
                "gh CLI not found - install GitHub CLI to create PRs".to_string(),
            ));
        }

        let config = SubmitConfig::default(); // TODO: pass config through
        let target_branch = &config.git.target_branch;

        // Build PR body
        let body = self.build_pr_body(goal, pr, &config)?;

        tracing::info!("GitAdapter: creating PR to {}", target_branch);

        // Create PR using gh CLI
        let output = Command::new("gh")
            .args([
                "pr",
                "create",
                "--base",
                target_branch,
                "--title",
                &goal.title,
                "--body",
                &body,
            ])
            .current_dir(&self.work_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SubmitError::ReviewError(format!(
                "gh pr create failed: {}",
                stderr
            )));
        }

        let pr_url = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Extract PR number from URL (e.g., https://github.com/owner/repo/pull/123)
        let pr_number = pr_url
            .split('/')
            .next_back()
            .unwrap_or("unknown")
            .to_string();

        Ok(ReviewResult {
            review_url: pr_url.clone(),
            review_id: pr_number,
            message: format!("Created PR: {}", pr_url),
            metadata: [("pr_url".to_string(), pr_url)].into_iter().collect(),
        })
    }

    fn name(&self) -> &str {
        "git"
    }
}

impl GitAdapter {
    /// Build PR body from template or default format
    fn build_pr_body(
        &self,
        goal: &GoalRun,
        pr: &DraftPackage,
        config: &SubmitConfig,
    ) -> Result<String> {
        // Try to load template if specified
        if let Some(template_path) = &config.git.pr_template {
            if template_path.exists() {
                let template = std::fs::read_to_string(template_path)?;
                return Ok(self.substitute_template(&template, goal, pr));
            }
        }

        // Default PR body format
        Ok(format!(
            "## Summary\n\n{}\n\n**Why**: {}\n\n## Changes\n\n{} artifact(s) modified\n\n## Goal Context\n\n- **Goal ID**: `{}`\n- **PR ID**: `{}`\n{}\n\n---\n\n🤖 Generated by [Trusted Autonomy](https://github.com/trustedautonomy/ta)",
            pr.summary.what_changed,
            pr.summary.why,
            pr.changes.artifacts.len(),
            goal.goal_run_id,
            pr.package_id,
            goal.plan_phase.as_ref().map(|p| format!("- **Plan Phase**: `{}`", p)).unwrap_or_default()
        ))
    }

    /// Substitute template variables.
    ///
    /// Available variables:
    ///   {title}          — goal title
    ///   {summary}        — what changed (from change_summary.json)
    ///   {why}            — why it changed
    ///   {impact}         — impact assessment
    ///   {objective}      — full goal objective text
    ///   {artifact_count} — number of files changed
    ///   {artifacts}      — one line per artifact: "ChangeType  uri"
    ///   {goal_id}        — goal UUID
    ///   {pr_id}          — PR package UUID
    ///   {plan_phase}     — plan phase (or "N/A")
    fn substitute_template(&self, template: &str, goal: &GoalRun, pr: &DraftPackage) -> String {
        let artifact_lines: String = pr
            .changes
            .artifacts
            .iter()
            .map(|a| format!("- `{:?}` {}", a.change_type, a.resource_uri))
            .collect::<Vec<_>>()
            .join("\n");

        template
            .replace("{summary}", &pr.summary.what_changed)
            .replace("{why}", &pr.summary.why)
            .replace("{impact}", &pr.summary.impact)
            .replace("{goal_id}", &goal.goal_run_id.to_string())
            .replace("{pr_id}", &pr.package_id.to_string())
            .replace("{title}", &goal.title)
            .replace("{objective}", &goal.objective)
            .replace("{plan_phase}", goal.plan_phase.as_deref().unwrap_or("N/A"))
            .replace("{artifact_count}", &pr.changes.artifacts.len().to_string())
            .replace("{artifacts}", &artifact_lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    fn init_git_repo(dir: &Path) -> Result<()> {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()?;
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir)
            .output()?;
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .output()?;

        // Create initial commit
        std::fs::write(dir.join("README.md"), "# Test\n")?;
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(dir)
            .output()?;

        Ok(())
    }

    #[test]
    fn test_git_adapter_branch_name() {
        let dir = tempdir().unwrap();
        init_git_repo(dir.path()).unwrap();

        let adapter = GitAdapter::new(dir.path());
        let goal = GoalRun::new(
            "Add New Feature",
            "Test",
            "test-agent",
            dir.path().to_path_buf(),
            dir.path().join("store"),
        );

        let config = SubmitConfig::default();
        let branch = adapter.branch_name(&goal, &config);

        assert!(branch.starts_with("ta/"));
        assert!(branch.contains("add-new-feature"));
    }

    #[test]
    fn test_git_adapter_prepare() {
        let dir = tempdir().unwrap();
        init_git_repo(dir.path()).unwrap();

        let adapter = GitAdapter::new(dir.path());
        let goal = GoalRun::new(
            "Test Goal",
            "Test",
            "test-agent",
            dir.path().to_path_buf(),
            dir.path().join("store"),
        );

        let config = SubmitConfig::default();
        assert!(adapter.prepare(&goal, &config).is_ok());

        // Verify we're on the new branch
        let current = adapter.current_branch().unwrap();
        assert!(current.starts_with("ta/"));
    }
}
