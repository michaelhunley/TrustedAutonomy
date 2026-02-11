// pr.rs — PR package subcommands: build, list, view, approve, deny, apply.

use std::fs;

use chrono::Utc;
use clap::Subcommand;
use ta_changeset::changeset::{ChangeKind, ChangeSet, CommitIntent};
use ta_changeset::diff::DiffContent;
use ta_changeset::pr_package::{
    AgentIdentity, Artifact, ChangeType, Changes, Goal, Iteration, PRPackage, PRStatus, Plan,
    Provenance, RequestedAction, ReviewRequests, Risk, Signatures, Summary, WorkspaceRef,
};
use ta_connector_fs::FsConnector;
use ta_goal::{GoalRunState, GoalRunStore};
use ta_mcp_gateway::GatewayConfig;
use ta_workspace::{
    ChangeStore, ExcludePatterns, JsonFileStore, OverlayWorkspace, StagingWorkspace,
};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum PrCommands {
    /// Build a PR package from overlay workspace diffs.
    Build {
        /// Goal run ID.
        goal_id: String,
        /// Summary of what changed and why.
        #[arg(long, default_value = "Changes from agent work")]
        summary: String,
    },
    /// List all PR packages.
    List {
        /// Filter by goal run ID.
        #[arg(long)]
        goal: Option<String>,
    },
    /// View PR package details and diffs.
    View {
        /// PR package ID.
        id: String,
    },
    /// Approve a PR package for application.
    Approve {
        /// PR package ID.
        id: String,
        /// Reviewer name.
        #[arg(long, default_value = "human-reviewer")]
        reviewer: String,
    },
    /// Deny a PR package with a reason.
    Deny {
        /// PR package ID.
        id: String,
        /// Reason for denial.
        #[arg(long)]
        reason: String,
        /// Reviewer name.
        #[arg(long, default_value = "human-reviewer")]
        reviewer: String,
    },
    /// Apply approved changes to the target directory.
    Apply {
        /// PR package ID.
        id: String,
        /// Target directory (defaults to project root).
        #[arg(long)]
        target: Option<String>,
        /// Create a git commit after applying.
        #[arg(long)]
        git_commit: bool,
        /// Push to remote after committing (implies --git-commit).
        #[arg(long)]
        git_push: bool,
    },
}

pub fn execute(cmd: &PrCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match cmd {
        PrCommands::Build { goal_id, summary } => build_package(config, goal_id, summary),
        PrCommands::List { goal } => list_packages(config, goal.as_deref()),
        PrCommands::View { id } => view_package(config, id),
        PrCommands::Approve { id, reviewer } => approve_package(config, id, reviewer),
        PrCommands::Deny {
            id,
            reason,
            reviewer,
        } => deny_package(config, id, reason, reviewer),
        PrCommands::Apply {
            id,
            target,
            git_commit,
            git_push,
        } => apply_package(
            config,
            id,
            target.as_deref(),
            *git_commit || *git_push,
            *git_push,
        ),
    }
}

fn build_package(config: &GatewayConfig, goal_id: &str, summary: &str) -> anyhow::Result<()> {
    let goal_uuid = Uuid::parse_str(goal_id)?;
    let goal_store = GoalRunStore::new(&config.goals_dir)?;

    let goal = goal_store
        .get(goal_uuid)?
        .ok_or_else(|| anyhow::anyhow!("Goal run not found: {}", goal_id))?;

    if !matches!(goal.state, GoalRunState::Running) {
        anyhow::bail!(
            "Goal is in {} state (must be running to build PR)",
            goal.state
        );
    }

    let source_dir = goal
        .source_dir
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Goal has no source_dir (not an overlay-based goal)"))?;

    // Open the overlay workspace and compute diffs.
    // V1 TEMPORARY: Load exclude patterns for diff filtering.
    let excludes = ExcludePatterns::load(source_dir);
    let overlay = OverlayWorkspace::open(goal_id, source_dir, &goal.workspace_path, excludes);
    let changes = overlay.diff_all().map_err(|e| anyhow::anyhow!("{}", e))?;

    if changes.is_empty() {
        anyhow::bail!("No changes detected in staging workspace");
    }

    // Convert overlay changes to PR package artifacts.
    let mut artifacts = Vec::new();
    let mut changesets = Vec::new();

    for change in &changes {
        match change {
            ta_workspace::overlay::OverlayChange::Modified { path, diff } => {
                artifacts.push(Artifact {
                    resource_uri: format!("fs://workspace/{}", path),
                    change_type: ChangeType::Modify,
                    diff_ref: format!("changeset:{}", changesets.len()),
                    tests_run: vec![],
                });
                changesets.push(
                    ChangeSet::new(
                        format!("fs://workspace/{}", path),
                        ChangeKind::FsPatch,
                        DiffContent::UnifiedDiff {
                            content: diff.clone(),
                        },
                    )
                    .with_commit_intent(CommitIntent::RequestCommit),
                );
            }
            ta_workspace::overlay::OverlayChange::Created { path, content } => {
                artifacts.push(Artifact {
                    resource_uri: format!("fs://workspace/{}", path),
                    change_type: ChangeType::Add,
                    diff_ref: format!("changeset:{}", changesets.len()),
                    tests_run: vec![],
                });
                changesets.push(
                    ChangeSet::new(
                        format!("fs://workspace/{}", path),
                        ChangeKind::FsPatch,
                        DiffContent::CreateFile {
                            content: content.clone(),
                        },
                    )
                    .with_commit_intent(CommitIntent::RequestCommit),
                );
            }
            ta_workspace::overlay::OverlayChange::Deleted { path } => {
                artifacts.push(Artifact {
                    resource_uri: format!("fs://workspace/{}", path),
                    change_type: ChangeType::Delete,
                    diff_ref: format!("changeset:{}", changesets.len()),
                    tests_run: vec![],
                });
                changesets.push(
                    ChangeSet::new(
                        format!("fs://workspace/{}", path),
                        ChangeKind::FsPatch,
                        DiffContent::DeleteFile,
                    )
                    .with_commit_intent(CommitIntent::RequestCommit),
                );
            }
        }
    }

    // Persist changesets to the store.
    let mut store = JsonFileStore::new(goal.store_path.clone())?;
    for cs in &changesets {
        store.save(goal_id, cs)?;
    }

    // Build the PR package.
    let package_id = Uuid::new_v4();
    let pkg = PRPackage {
        package_version: "1.0.0".to_string(),
        package_id,
        created_at: Utc::now(),
        goal: Goal {
            goal_id: goal_id.to_string(),
            title: goal.title.clone(),
            objective: goal.objective.clone(),
            success_criteria: vec![],
            constraints: vec![],
        },
        iteration: Iteration {
            iteration_id: format!("{}-1", goal_id),
            sequence: 1,
            workspace_ref: WorkspaceRef {
                ref_type: "overlay_staging".to_string(),
                ref_name: goal.workspace_path.display().to_string(),
                base_ref: Some(source_dir.display().to_string()),
            },
        },
        agent_identity: AgentIdentity {
            agent_id: goal.agent_id.clone(),
            agent_type: "coding".to_string(),
            constitution_id: "default".to_string(),
            capability_manifest_hash: goal.manifest_id.to_string(),
            orchestrator_run_id: None,
        },
        summary: Summary {
            what_changed: summary.to_string(),
            why: goal.objective.clone(),
            impact: format!("{} file(s) changed", artifacts.len()),
            rollback_plan: "Revert changes from staging".to_string(),
            open_questions: vec![],
        },
        plan: Plan {
            completed_steps: vec!["Agent completed work in staging".to_string()],
            next_steps: vec!["Review and apply changes".to_string()],
            decision_log: vec![],
        },
        changes: Changes {
            artifacts,
            patch_sets: vec![],
        },
        risk: Risk {
            risk_score: 0,
            findings: vec![],
            policy_decisions: vec![],
        },
        provenance: Provenance {
            inputs: vec![],
            tool_trace_hash: "overlay-diff".to_string(),
        },
        review_requests: ReviewRequests {
            requested_actions: vec![RequestedAction {
                action: "approve".to_string(),
                targets: vec!["all".to_string()],
            }],
            reviewers: vec!["human-reviewer".to_string()],
            required_approvals: 1,
            notes_to_reviewer: None,
        },
        signatures: Signatures {
            package_hash: "pending".to_string(),
            agent_signature: "pending".to_string(),
            gateway_attestation: None,
        },
        status: PRStatus::PendingReview,
    };

    // Save the PR package.
    save_package(config, &pkg)?;

    // Update the goal run.
    let mut goal = goal;
    goal.pr_package_id = Some(package_id);
    goal_store.save(&goal)?;
    goal_store.transition(goal_uuid, GoalRunState::PrReady)?;

    println!("PR package built: {}", package_id);
    println!("  Goal:    {} ({})", goal.title, goal_id);
    println!("  Changes: {} file(s)", pkg.changes.artifacts.len());
    for artifact in &pkg.changes.artifacts {
        println!("    {:?}  {}", artifact.change_type, artifact.resource_uri);
    }
    println!();
    println!("Review with:  ta pr view {}", package_id);
    println!("Approve with: ta pr approve {}", package_id);

    Ok(())
}

fn list_packages(config: &GatewayConfig, goal_filter: Option<&str>) -> anyhow::Result<()> {
    let packages = load_all_packages(config)?;

    let filtered: Vec<&PRPackage> = if let Some(goal_id) = goal_filter {
        packages
            .iter()
            .filter(|p| p.goal.goal_id == goal_id)
            .collect()
    } else {
        packages.iter().collect()
    };

    if filtered.is_empty() {
        println!("No PR packages found.");
        return Ok(());
    }

    println!(
        "{:<38} {:<30} {:<16} {:<8}",
        "PACKAGE ID", "GOAL", "STATUS", "FILES"
    );
    println!("{}", "-".repeat(92));

    for pkg in &filtered {
        println!(
            "{:<38} {:<30} {:<16} {:<8}",
            pkg.package_id,
            truncate(&pkg.goal.title, 28),
            format!("{:?}", pkg.status),
            pkg.changes.artifacts.len(),
        );
    }
    println!("\n{} package(s) total.", filtered.len());
    Ok(())
}

fn view_package(config: &GatewayConfig, id: &str) -> anyhow::Result<()> {
    let package_id = Uuid::parse_str(id)?;
    let pkg = load_package(config, package_id)?;

    println!("PR Package: {}", pkg.package_id);
    println!("Goal:       {} ({})", pkg.goal.title, pkg.goal.goal_id);
    println!("Status:     {:?}", pkg.status);
    println!("Created:    {}", pkg.created_at.to_rfc3339());
    println!(
        "Agent:      {} ({})",
        pkg.agent_identity.agent_id, pkg.agent_identity.agent_type
    );
    println!();
    println!("Summary");
    println!("  What: {}", pkg.summary.what_changed);
    println!("  Why:  {}", pkg.summary.why);
    println!("  Impact: {}", pkg.summary.impact);
    println!();
    println!("Changes ({} file(s)):", pkg.changes.artifacts.len());
    for artifact in &pkg.changes.artifacts {
        println!("  {:?}  {}", artifact.change_type, artifact.resource_uri);
    }
    println!();
    println!(
        "Review: {} approval(s) required from {:?}",
        pkg.review_requests.required_approvals, pkg.review_requests.reviewers
    );

    // Show diffs from staging if available.
    let goal_store = GoalRunStore::new(&config.goals_dir)?;
    let goals = goal_store.list()?;
    let matching_goal = goals.iter().find(|g| {
        g.goal_run_id.to_string() == pkg.goal.goal_id || g.pr_package_id == Some(pkg.package_id)
    });

    if let Some(goal) = matching_goal {
        let staging = StagingWorkspace::new(goal.goal_run_id.to_string(), &config.staging_dir)?;
        let staged_files = staging.list_files()?;

        if !staged_files.is_empty() {
            println!("\nDiffs:");
            println!("{}", "=".repeat(60));
            for file in &staged_files {
                if let Some(diff) = staging.diff_file(file)? {
                    println!("--- {}", file);
                    println!("{}", diff);
                    println!();
                }
            }
        }
    }

    Ok(())
}

fn approve_package(config: &GatewayConfig, id: &str, reviewer: &str) -> anyhow::Result<()> {
    let package_id = Uuid::parse_str(id)?;
    let mut pkg = load_package(config, package_id)?;

    if !matches!(pkg.status, PRStatus::PendingReview) {
        anyhow::bail!(
            "Cannot approve package in {:?} state (must be PendingReview)",
            pkg.status
        );
    }

    pkg.status = PRStatus::Approved {
        approved_by: reviewer.to_string(),
        approved_at: Utc::now(),
    };
    save_package(config, &pkg)?;

    // Transition the goal if we can find it.
    let goal_store = GoalRunStore::new(&config.goals_dir)?;
    let goals = goal_store.list()?;
    if let Some(goal) = goals.iter().find(|g| g.pr_package_id == Some(package_id)) {
        let _ = goal_store.transition(goal.goal_run_id, GoalRunState::UnderReview);
        let _ = goal_store.transition(
            goal.goal_run_id,
            GoalRunState::Approved {
                approved_by: reviewer.to_string(),
            },
        );
    }

    println!("Approved PR package {} by {}", package_id, reviewer);
    Ok(())
}

fn deny_package(
    config: &GatewayConfig,
    id: &str,
    reason: &str,
    reviewer: &str,
) -> anyhow::Result<()> {
    let package_id = Uuid::parse_str(id)?;
    let mut pkg = load_package(config, package_id)?;

    if !matches!(pkg.status, PRStatus::PendingReview) {
        anyhow::bail!(
            "Cannot deny package in {:?} state (must be PendingReview)",
            pkg.status
        );
    }

    pkg.status = PRStatus::Denied {
        reason: reason.to_string(),
        denied_by: reviewer.to_string(),
    };
    save_package(config, &pkg)?;

    println!("Denied PR package {}: {}", package_id, reason);
    Ok(())
}

fn apply_package(
    config: &GatewayConfig,
    id: &str,
    target: Option<&str>,
    git_commit: bool,
    git_push: bool,
) -> anyhow::Result<()> {
    let package_id = Uuid::parse_str(id)?;
    let mut pkg = load_package(config, package_id)?;

    if !matches!(pkg.status, PRStatus::Approved { .. }) {
        anyhow::bail!(
            "Cannot apply package in {:?} state (must be Approved)",
            pkg.status
        );
    }

    // Find the goal for this package.
    let goal_store = GoalRunStore::new(&config.goals_dir)?;
    let goals = goal_store.list()?;
    let goal = goals
        .iter()
        .find(|g| g.pr_package_id == Some(package_id))
        .ok_or_else(|| anyhow::anyhow!("No goal found for PR package {}", package_id))?;

    let target_dir = match target {
        Some(t) => std::path::PathBuf::from(t),
        None => goal
            .source_dir
            .clone()
            .unwrap_or_else(|| config.workspace_root.clone()),
    };

    // Apply changes — use overlay path for overlay-based goals, legacy path otherwise.
    let applied_files: Vec<String> = if let Some(ref source_dir) = goal.source_dir {
        // Overlay-based goal: diff staging vs source, copy changed files.
        // V1 TEMPORARY: Load exclude patterns for diff filtering.
        let excludes = ExcludePatterns::load(source_dir);
        let overlay = OverlayWorkspace::open(
            goal.goal_run_id.to_string(),
            source_dir,
            &goal.workspace_path,
            excludes,
        );
        let applied = overlay
            .apply_to(&target_dir)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        applied
            .into_iter()
            .map(|(path, kind)| format!("{} ({})", path, kind))
            .collect()
    } else {
        // Legacy MCP-based goal: use FsConnector.
        let staging = StagingWorkspace::new(goal.goal_run_id.to_string(), &config.staging_dir)?;
        let store = JsonFileStore::new(config.store_dir.join(goal.goal_run_id.to_string()))?;
        let mut connector =
            FsConnector::new(goal.goal_run_id.to_string(), staging, store, &goal.agent_id);
        connector.apply(&target_dir)?
    };

    println!(
        "Applied {} file(s) to {}",
        applied_files.len(),
        target_dir.display()
    );
    for file in &applied_files {
        println!("  {}", file);
    }

    // Git integration.
    if git_commit {
        println!("\nCreating git commit...");
        let commit_msg = format!(
            "{}\n\nApplied via Trusted Autonomy PR package {}",
            pkg.summary.what_changed, package_id
        );

        // git add all changed files.
        let add_result = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&target_dir)
            .output()?;

        if !add_result.status.success() {
            anyhow::bail!(
                "git add failed: {}",
                String::from_utf8_lossy(&add_result.stderr)
            );
        }

        // git commit.
        let commit_result = std::process::Command::new("git")
            .args(["commit", "-m", &commit_msg])
            .current_dir(&target_dir)
            .output()?;

        if !commit_result.status.success() {
            let stderr = String::from_utf8_lossy(&commit_result.stderr);
            if stderr.contains("nothing to commit") {
                println!("No changes to commit (already up to date).");
            } else {
                anyhow::bail!("git commit failed: {}", stderr);
            }
        } else {
            println!(
                "Committed: {}",
                String::from_utf8_lossy(&commit_result.stdout).trim()
            );
        }

        if git_push {
            println!("Pushing to remote...");
            let push_result = std::process::Command::new("git")
                .args(["push"])
                .current_dir(&target_dir)
                .output()?;

            if !push_result.status.success() {
                anyhow::bail!(
                    "git push failed: {}",
                    String::from_utf8_lossy(&push_result.stderr)
                );
            }
            println!("Pushed successfully.");
        }
    }

    // Transition goal to Applied → update package status.
    let _ = goal_store.transition(goal.goal_run_id, GoalRunState::Applied);
    pkg.status = PRStatus::Applied {
        applied_at: Utc::now(),
    };
    save_package(config, &pkg)?;

    Ok(())
}

// ── File-based PR package storage ────────────────────────────────

fn load_all_packages(config: &GatewayConfig) -> anyhow::Result<Vec<PRPackage>> {
    let dir = &config.pr_packages_dir;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut packages = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let json = fs::read_to_string(&path)?;
            if let Ok(pkg) = serde_json::from_str::<PRPackage>(&json) {
                packages.push(pkg);
            }
        }
    }

    packages.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(packages)
}

fn load_package(config: &GatewayConfig, package_id: Uuid) -> anyhow::Result<PRPackage> {
    let path = config.pr_packages_dir.join(format!("{}.json", package_id));
    if !path.exists() {
        anyhow::bail!("PR package not found: {}", package_id);
    }
    let json = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&json)?)
}

fn save_package(config: &GatewayConfig, pkg: &PRPackage) -> anyhow::Result<()> {
    fs::create_dir_all(&config.pr_packages_dir)?;
    let path = config
        .pr_packages_dir
        .join(format!("{}.json", pkg.package_id));
    let json = serde_json::to_string_pretty(pkg)?;
    fs::write(&path, json)?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max - 3])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn build_pr_from_overlay_changes() {
        // Set up a source project.
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Original\n").unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

        let config = GatewayConfig::for_project(project.path());
        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();

        // Start a goal.
        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Test PR build".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test PR building from overlay".to_string(),
                agent: "test-agent".to_string(),
            },
            &config,
        )
        .unwrap();

        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        // Make changes in the staging workspace.
        std::fs::write(
            goal.workspace_path.join("README.md"),
            "# Modified Project\n",
        )
        .unwrap();
        std::fs::write(
            goal.workspace_path.join("src/new_file.rs"),
            "pub fn new_thing() {}\n",
        )
        .unwrap();
        std::fs::remove_file(goal.workspace_path.join("src/lib.rs")).unwrap();

        // Build PR package.
        build_package(&config, &goal_id, "Test changes").unwrap();

        // Verify PR package was created.
        let packages = load_all_packages(&config).unwrap();
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.status, PRStatus::PendingReview);
        assert_eq!(pkg.changes.artifacts.len(), 3);

        // Verify artifact types.
        let types: Vec<&ChangeType> = pkg
            .changes
            .artifacts
            .iter()
            .map(|a| &a.change_type)
            .collect();
        assert!(types.contains(&&ChangeType::Modify));
        assert!(types.contains(&&ChangeType::Add));
        assert!(types.contains(&&ChangeType::Delete));

        // Verify goal state transitioned to PrReady.
        let updated_goal = goal_store.get(goal.goal_run_id).unwrap().unwrap();
        assert_eq!(updated_goal.state, GoalRunState::PrReady);
        assert!(updated_goal.pr_package_id.is_some());
    }

    #[test]
    fn apply_overlay_copies_changes_to_source() {
        // Set up a source project.
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Original\n").unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n").unwrap();

        let config = GatewayConfig::for_project(project.path());

        // Start goal + modify + build PR + approve + apply.
        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Apply test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test apply".to_string(),
                agent: "test-agent".to_string(),
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        // Make changes in staging.
        std::fs::write(goal.workspace_path.join("README.md"), "# Updated\n").unwrap();
        std::fs::write(goal.workspace_path.join("NEW.md"), "new file\n").unwrap();

        // Build PR.
        build_package(&config, &goal_id, "Test apply changes").unwrap();

        // Approve the PR.
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();
        approve_package(&config, &pkg_id, "tester").unwrap();

        // Apply (no git).
        apply_package(&config, &pkg_id, None, false, false).unwrap();

        // Verify files changed in source.
        let readme = std::fs::read_to_string(project.path().join("README.md")).unwrap();
        assert_eq!(readme, "# Updated\n");
        let new_file = std::fs::read_to_string(project.path().join("NEW.md")).unwrap();
        assert_eq!(new_file, "new file\n");

        // Verify goal state.
        let updated = goal_store.get(goal.goal_run_id).unwrap().unwrap();
        assert_eq!(updated.state, GoalRunState::Applied);
    }

    #[test]
    fn apply_with_git_commit() {
        // Set up a git repo as source.
        let project = TempDir::new().unwrap();

        // Initialize git repo.
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(project.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(project.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(project.path())
            .output()
            .unwrap();

        std::fs::write(project.path().join("README.md"), "# Test\n").unwrap();

        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(project.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(project.path())
            .output()
            .unwrap();

        let config = GatewayConfig::for_project(project.path());

        // Start goal.
        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Git test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test git commit".to_string(),
                agent: "test-agent".to_string(),
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        // Modify file in staging.
        std::fs::write(goal.workspace_path.join("README.md"), "# Modified\n").unwrap();

        // Build + approve + apply with git commit.
        build_package(&config, &goal_id, "Modified README").unwrap();
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();
        approve_package(&config, &pkg_id, "tester").unwrap();
        apply_package(&config, &pkg_id, None, true, false).unwrap();

        // Verify git log has new commit.
        let log = std::process::Command::new("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(project.path())
            .output()
            .unwrap();
        let log_output = String::from_utf8_lossy(&log.stdout);
        assert!(log_output.contains("Modified README"));
    }

    #[test]
    fn build_pr_with_no_changes_fails() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Test\n").unwrap();

        let config = GatewayConfig::for_project(project.path());

        // Start a goal.
        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "No changes".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Nothing".to_string(),
                agent: "test-agent".to_string(),
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal_id = goals[0].goal_run_id.to_string();

        // Build PR should fail — no changes.
        let result = build_package(&config, &goal_id, "No changes");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No changes"));
    }
}
