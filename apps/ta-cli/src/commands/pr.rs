// pr.rs — PR package subcommands: build, list, view, approve, deny, apply.

use std::fs;

use chrono::Utc;
use clap::Subcommand;
use ta_changeset::changeset::{ChangeKind, ChangeSet, CommitIntent};
use ta_changeset::diff::DiffContent;
use ta_changeset::pr_package::{
    AgentIdentity, Artifact, ArtifactDisposition, ChangeDependency, ChangeType, Changes,
    DependencyKind, Goal, Iteration, PRPackage, PRStatus, Plan, Provenance, RequestedAction,
    ReviewRequests, Risk, Signatures, Summary, WorkspaceRef,
};
use ta_changeset::uri_pattern;
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
        /// Goal run ID (omit with --latest to use most recent running goal).
        #[arg(default_value = "")]
        goal_id: String,
        /// Summary of what changed and why.
        #[arg(long, default_value = "Changes from agent work")]
        summary: String,
        /// Use the most recent running goal instead of specifying an ID.
        #[arg(long)]
        latest: bool,
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
        /// Show summary and file list only (skip diffs).
        #[arg(long)]
        summary: bool,
        /// Show diff for a single file only (path relative to workspace root).
        #[arg(long)]
        file: Option<String>,
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
        /// Run full submit workflow (commit + push + open review).
        /// Equivalent to --git-commit --git-push with auto PR creation.
        #[arg(long)]
        submit: bool,
        /// Approve artifacts matching these patterns (repeatable).
        /// Special values: "all" (everything), "rest" (everything not explicitly matched).
        #[arg(long = "approve")]
        approve_patterns: Vec<String>,
        /// Reject artifacts matching these patterns (repeatable).
        #[arg(long = "reject")]
        reject_patterns: Vec<String>,
        /// Mark artifacts for discussion matching these patterns (repeatable).
        #[arg(long = "discuss")]
        discuss_patterns: Vec<String>,
    },
}

pub fn execute(cmd: &PrCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match cmd {
        PrCommands::Build {
            goal_id,
            summary,
            latest,
        } => build_package(config, goal_id, summary, *latest),
        PrCommands::List { goal } => list_packages(config, goal.as_deref()),
        PrCommands::View { id, summary, file } => {
            view_package(config, id, *summary, file.as_deref())
        }
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
            submit,
            approve_patterns,
            reject_patterns,
            discuss_patterns,
        } => {
            // Load workflow config to merge auto_* settings with CLI flags.
            let workflow_config = ta_submit::WorkflowConfig::load_or_default(
                &config.workspace_root.join(".ta/workflow.toml"),
            );

            // CLI flags override config. --submit implies full workflow.
            let do_commit =
                *git_commit || *git_push || *submit || workflow_config.submit.auto_commit;
            let do_push = *git_push || *submit || workflow_config.submit.auto_push;
            let do_review = *submit || workflow_config.submit.auto_review;

            apply_package(
                config,
                id,
                target.as_deref(),
                do_commit,
                do_push,
                do_review,
                SelectiveReviewPatterns {
                    approve: approve_patterns,
                    reject: reject_patterns,
                    discuss: discuss_patterns,
                },
            )
        }
    }
}

// ── Agent-generated change summary (.ta/change_summary.json) ──

/// Agent-provided change summary with per-file rationale and dependency info.
/// Written by the agent to `.ta/change_summary.json` before exiting.
#[derive(Debug, serde::Deserialize)]
struct ChangeSummary {
    summary: Option<String>,
    #[serde(default)]
    changes: Vec<ChangeSummaryEntry>,
    dependency_notes: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ChangeSummaryEntry {
    path: String,
    #[allow(dead_code)]
    action: Option<String>,
    why: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    independent: bool,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    depended_by: Vec<String>,
}

/// Try to load the agent's change summary from the staging workspace.
fn load_change_summary(staging_path: &std::path::Path) -> Option<ChangeSummary> {
    let path = staging_path.join(".ta/change_summary.json");
    let content = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<ChangeSummary>(&content) {
        Ok(summary) => Some(summary),
        Err(e) => {
            eprintln!("Warning: could not parse .ta/change_summary.json: {}", e);
            None
        }
    }
}

/// Look up a change summary entry by path and populate artifact fields.
fn enrich_artifact(artifact: &mut Artifact, summary: &ChangeSummary) {
    // Extract the relative path from fs://workspace/<path>.
    let rel_path = artifact
        .resource_uri
        .strip_prefix("fs://workspace/")
        .unwrap_or(&artifact.resource_uri);

    if let Some(entry) = summary.changes.iter().find(|c| c.path == rel_path) {
        artifact.rationale = entry.why.clone();

        for dep_path in &entry.depends_on {
            artifact.dependencies.push(ChangeDependency {
                target_uri: format!("fs://workspace/{}", dep_path),
                kind: DependencyKind::DependsOn,
            });
        }
        for dep_path in &entry.depended_by {
            artifact.dependencies.push(ChangeDependency {
                target_uri: format!("fs://workspace/{}", dep_path),
                kind: DependencyKind::DependedBy,
            });
        }
    }
}

fn build_package(
    config: &GatewayConfig,
    goal_id: &str,
    summary: &str,
    latest: bool,
) -> anyhow::Result<()> {
    let goal_store = GoalRunStore::new(&config.goals_dir)?;

    // Resolve the goal — either by ID or by finding the latest running goal.
    let goal = if latest || goal_id.is_empty() {
        let goals = goal_store.list()?;
        goals
            .into_iter()
            .find(|g| matches!(g.state, GoalRunState::Running))
            .ok_or_else(|| {
                anyhow::anyhow!("No running goal found (use a goal ID or start a goal first)")
            })?
    } else {
        let goal_uuid = Uuid::parse_str(goal_id)?;
        goal_store
            .get(goal_uuid)?
            .ok_or_else(|| anyhow::anyhow!("Goal run not found: {}", goal_id))?
    };
    let goal_id = goal.goal_run_id.to_string();

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
    let overlay =
        OverlayWorkspace::open(goal_id.clone(), source_dir, &goal.workspace_path, excludes);
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
                    disposition: Default::default(),
                    rationale: None,
                    dependencies: vec![],
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
                    disposition: Default::default(),
                    rationale: None,
                    dependencies: vec![],
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
                    disposition: Default::default(),
                    rationale: None,
                    dependencies: vec![],
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
        store.save(&goal_id, cs)?;
    }

    // Enrich artifacts with agent-provided rationale and dependency info.
    let change_summary = load_change_summary(&goal.workspace_path);
    if let Some(ref cs) = change_summary {
        for artifact in &mut artifacts {
            enrich_artifact(artifact, cs);
        }
        let enriched_count = artifacts.iter().filter(|a| a.rationale.is_some()).count();
        if enriched_count > 0 {
            println!(
                "Loaded change_summary.json: {}/{} artifacts enriched with rationale",
                enriched_count,
                artifacts.len()
            );
        }
    }

    // Use agent summary if available and user didn't provide a custom one.
    let effective_summary = if summary == "Changes from agent work" {
        change_summary
            .as_ref()
            .and_then(|cs| cs.summary.clone())
            .unwrap_or_else(|| summary.to_string())
    } else {
        summary.to_string()
    };

    let dependency_notes = change_summary
        .as_ref()
        .and_then(|cs| cs.dependency_notes.clone());

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
            what_changed: effective_summary,
            why: goal.objective.clone(),
            impact: format!("{} file(s) changed", artifacts.len()),
            rollback_plan: "Revert changes from staging".to_string(),
            open_questions: dependency_notes.into_iter().collect(),
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

    // Handle PR supersession for follow-up goals.
    // If this goal has a parent and the parent's PR is not yet applied,
    // mark the parent PR as superseded by this new PR.
    if let Some(parent_goal_id) = goal.parent_goal_id {
        if let Some(parent_goal) = goal_store.get(parent_goal_id)? {
            if let Some(parent_pr_id) = parent_goal.pr_package_id {
                // Load parent PR and check if it's unapplied.
                if let Ok(mut parent_pr) = load_package(config, parent_pr_id) {
                    match parent_pr.status {
                        PRStatus::Draft | PRStatus::PendingReview | PRStatus::Approved { .. } => {
                            // Parent PR not yet applied — mark it as superseded.
                            parent_pr.status = PRStatus::Superseded {
                                superseded_by: package_id,
                            };
                            save_package(config, &parent_pr)?;
                            println!(
                                "Parent PR {} superseded by this follow-up PR.",
                                parent_pr_id
                            );
                        }
                        PRStatus::Applied { .. } | PRStatus::Denied { .. } => {
                            // Parent already applied or denied — no supersession needed.
                        }
                        PRStatus::Superseded { .. } => {
                            // Parent already superseded — nothing to do.
                        }
                    }
                }
            }
        }
    }

    // Save the PR package.
    save_package(config, &pkg)?;

    // Update the goal run.
    let mut goal = goal;
    goal.pr_package_id = Some(package_id);
    goal_store.save(&goal)?;
    goal_store.transition(goal.goal_run_id, GoalRunState::PrReady)?;

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
        let status_display = match &pkg.status {
            PRStatus::Superseded { superseded_by } => {
                format!("superseded ({})", &superseded_by.to_string()[..8])
            }
            _ => format!("{:?}", pkg.status),
        };

        println!(
            "{:<38} {:<30} {:<16} {:<8}",
            pkg.package_id,
            truncate(&pkg.goal.title, 28),
            status_display,
            pkg.changes.artifacts.len(),
        );
    }
    println!("\n{} package(s) total.", filtered.len());
    Ok(())
}

/// Check if a file appears to be binary by looking for null bytes in the first 8KB.
fn is_binary_file(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 8192];
    let Ok(n) = file.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

/// Human-readable file size display.
fn file_size_display(path: &std::path::Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return "unknown size".to_string();
    };
    let bytes = meta.len();
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn view_package(
    config: &GatewayConfig,
    id: &str,
    summary_only: bool,
    file_filter: Option<&str>,
) -> anyhow::Result<()> {
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
    if !pkg.summary.open_questions.is_empty() {
        println!("  Notes: {}", pkg.summary.open_questions.join("; "));
    }
    println!();
    println!("Changes ({} file(s)):", pkg.changes.artifacts.len());
    for artifact in &pkg.changes.artifacts {
        println!("  {:?}  {}", artifact.change_type, artifact.resource_uri);
        if let Some(ref rationale) = artifact.rationale {
            println!("         Why: {}", rationale);
        }
        if !artifact.dependencies.is_empty() {
            for dep in &artifact.dependencies {
                println!("         {:?}: {}", dep.kind, dep.target_uri);
            }
        }
    }
    println!();
    println!(
        "Review: {} approval(s) required from {:?}",
        pkg.review_requests.required_approvals, pkg.review_requests.reviewers
    );

    // Skip diffs if --summary was passed.
    if summary_only {
        return Ok(());
    }

    // Show diffs from staging if available.
    let goal_store = GoalRunStore::new(&config.goals_dir)?;
    let goals = goal_store.list()?;
    let matching_goal = goals.iter().find(|g| {
        g.goal_run_id.to_string() == pkg.goal.goal_id || g.pr_package_id == Some(pkg.package_id)
    });

    if let Some(goal) = matching_goal {
        // Use artifact URIs from the package (already filtered by ExcludePatterns)
        // instead of staging.list_files() which walks ALL files including target/.
        let artifact_files: Vec<String> = pkg
            .changes
            .artifacts
            .iter()
            .filter_map(|a| {
                a.resource_uri
                    .strip_prefix("fs://workspace/")
                    .map(String::from)
            })
            .collect();

        // Filter to a single file if --file was passed.
        let files_to_show: Vec<&String> = if let Some(filter) = file_filter {
            artifact_files
                .iter()
                .filter(|f| f.as_str() == filter)
                .collect()
        } else {
            artifact_files.iter().collect()
        };

        if let Some(filter) = file_filter {
            if files_to_show.is_empty() {
                println!("\nFile '{}' not found in staged changes.", filter);
                println!("Available files:");
                for f in &artifact_files {
                    println!("  {}", f);
                }
                return Ok(());
            }
        }

        if !files_to_show.is_empty() {
            let staging = StagingWorkspace::new(goal.goal_run_id.to_string(), &config.staging_dir)?;
            println!("\nDiffs:");
            println!("{}", "=".repeat(60));
            for file in &files_to_show {
                // Check for binary files.
                let staged_path = goal.workspace_path.join(file);
                if is_binary_file(&staged_path) {
                    let size = file_size_display(&staged_path);
                    println!("--- {}", file);
                    println!("[binary: {}]", size);
                    println!();
                    continue;
                }

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

/// Selective review patterns for artifact disposition.
#[derive(Default)]
struct SelectiveReviewPatterns<'a> {
    approve: &'a [String],
    reject: &'a [String],
    discuss: &'a [String],
}

impl<'a> SelectiveReviewPatterns<'a> {
    fn is_enabled(&self) -> bool {
        !self.approve.is_empty() || !self.reject.is_empty() || !self.discuss.is_empty()
    }
}

/// Validate that approved artifacts don't have broken dependencies.
///
/// Returns warnings for:
/// - Approved artifacts that depend on rejected artifacts
/// - Rejected artifacts that are depended upon by approved artifacts
///
/// Each warning includes the source artifact, target artifact, and dependency type.
fn validate_dependencies(artifacts: &[Artifact]) -> Vec<String> {
    let mut warnings = Vec::new();

    // Build a URI -> Artifact map for quick lookups.
    let artifact_map: std::collections::HashMap<&str, &Artifact> = artifacts
        .iter()
        .map(|a| (a.resource_uri.as_str(), a))
        .collect();

    for artifact in artifacts {
        if artifact.disposition != ArtifactDisposition::Approved {
            continue;
        }

        // Check if any dependencies are rejected.
        for dep in &artifact.dependencies {
            if dep.kind != DependencyKind::DependsOn {
                continue;
            }
            if let Some(target) = artifact_map.get(dep.target_uri.as_str()) {
                if target.disposition == ArtifactDisposition::Rejected {
                    warnings.push(format!(
                        "Warning: Approved artifact {} depends on rejected artifact {}",
                        artifact.resource_uri, target.resource_uri
                    ));
                } else if target.disposition == ArtifactDisposition::Pending {
                    warnings.push(format!(
                        "Warning: Approved artifact {} depends on pending artifact {} (not explicitly approved)",
                        artifact.resource_uri, target.resource_uri
                    ));
                }
            }
        }
    }

    // Check for rejected artifacts that are depended upon by approved artifacts.
    for artifact in artifacts {
        if artifact.disposition != ArtifactDisposition::Rejected {
            continue;
        }

        for dep in &artifact.dependencies {
            if dep.kind != DependencyKind::DependedBy {
                continue;
            }
            if let Some(dependent) = artifact_map.get(dep.target_uri.as_str()) {
                if dependent.disposition == ArtifactDisposition::Approved {
                    warnings.push(format!(
                        "Warning: Approved artifact {} will break because its dependency {} is rejected",
                        dependent.resource_uri, artifact.resource_uri
                    ));
                }
            }
        }
    }

    warnings
}

/// Assign dispositions to artifacts based on user-provided patterns.
///
/// Processing order:
/// 1. --approve patterns are applied first
/// 2. --reject patterns are applied second
/// 3. --discuss patterns are applied last
///
/// Special values:
/// - "all": matches all artifacts
/// - "rest": matches all artifacts not yet assigned a disposition
///
/// Returns the count of artifacts assigned to each disposition.
fn assign_dispositions(
    artifacts: &mut [Artifact],
    approve_patterns: &[String],
    reject_patterns: &[String],
    discuss_patterns: &[String],
) -> (usize, usize, usize) {
    // Helper: check if a pattern matches an artifact.
    let matches = |pattern: &str, artifact: &Artifact| -> bool {
        if pattern == "all" {
            true
        } else if pattern == "rest" {
            artifact.disposition == ArtifactDisposition::Pending
        } else {
            uri_pattern::matches_uri(pattern, &artifact.resource_uri)
        }
    };

    // Apply patterns in order: approve → reject → discuss.
    // Later patterns override earlier ones (last writer wins per artifact).
    for pattern in approve_patterns {
        for artifact in artifacts.iter_mut() {
            if matches(pattern, artifact) {
                artifact.disposition = ArtifactDisposition::Approved;
            }
        }
    }

    for pattern in reject_patterns {
        for artifact in artifacts.iter_mut() {
            if matches(pattern, artifact) {
                artifact.disposition = ArtifactDisposition::Rejected;
            }
        }
    }

    for pattern in discuss_patterns {
        for artifact in artifacts.iter_mut() {
            if matches(pattern, artifact) {
                artifact.disposition = ArtifactDisposition::Discuss;
            }
        }
    }

    // Count final dispositions from actual artifact state (not incrementally).
    let approved = artifacts
        .iter()
        .filter(|a| a.disposition == ArtifactDisposition::Approved)
        .count();
    let rejected = artifacts
        .iter()
        .filter(|a| a.disposition == ArtifactDisposition::Rejected)
        .count();
    let discussed = artifacts
        .iter()
        .filter(|a| a.disposition == ArtifactDisposition::Discuss)
        .count();

    (approved, rejected, discussed)
}

fn apply_package(
    config: &GatewayConfig,
    id: &str,
    target: Option<&str>,
    git_commit: bool,
    git_push: bool,
    git_review: bool,
    patterns: SelectiveReviewPatterns,
) -> anyhow::Result<()> {
    let package_id = Uuid::parse_str(id)?;
    let mut pkg = load_package(config, package_id)?;

    // Check if selective review is enabled.
    let selective_review = patterns.is_enabled();

    if selective_review {
        // Selective review mode: allow PendingReview or Approved packages.
        if !matches!(
            pkg.status,
            PRStatus::PendingReview | PRStatus::Approved { .. }
        ) {
            anyhow::bail!(
                "Cannot apply package in {:?} state (must be PendingReview or Approved for selective review)",
                pkg.status
            );
        }

        // Assign dispositions based on patterns.
        let (approved, rejected, discussed) = assign_dispositions(
            &mut pkg.changes.artifacts,
            patterns.approve,
            patterns.reject,
            patterns.discuss,
        );

        let pending = pkg
            .changes
            .artifacts
            .iter()
            .filter(|a| a.disposition == ArtifactDisposition::Pending)
            .count();

        println!("Selective review disposition summary:");
        println!("  Approved: {} artifact(s)", approved);
        println!("  Rejected: {} artifact(s)", rejected);
        println!("  Discuss:  {} artifact(s)", discussed);
        println!("  Pending:  {} artifact(s)", pending);
        println!();

        // Validate dependencies.
        let warnings = validate_dependencies(&pkg.changes.artifacts);
        if !warnings.is_empty() {
            println!("Dependency warnings:");
            for warning in &warnings {
                println!("  {}", warning);
            }
            println!();
            anyhow::bail!(
                "Cannot apply: {} dependency conflict(s) detected. Resolve conflicts and try again.",
                warnings.len()
            );
        }

        // Count approved artifacts.
        let approved_count = pkg
            .changes
            .artifacts
            .iter()
            .filter(|a| a.disposition == ArtifactDisposition::Approved)
            .count();

        if approved_count == 0 {
            anyhow::bail!("No artifacts approved for application");
        }

        println!("Applying {} approved artifact(s)...", approved_count);
    } else {
        // Legacy all-or-nothing mode: require Approved status.
        if !matches!(pkg.status, PRStatus::Approved { .. }) {
            anyhow::bail!(
                "Cannot apply package in {:?} state (must be Approved)",
                pkg.status
            );
        }
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

        let applied = if selective_review {
            // Selective mode: only apply approved artifacts.
            let approved_uris: Vec<String> = pkg
                .changes
                .artifacts
                .iter()
                .filter(|a| a.disposition == ArtifactDisposition::Approved)
                .map(|a| a.resource_uri.clone())
                .collect();
            overlay
                .apply_selective(&target_dir, &approved_uris)
                .map_err(|e| anyhow::anyhow!("{}", e))?
        } else {
            // Legacy mode: apply all changes.
            overlay
                .apply_to(&target_dir)
                .map_err(|e| anyhow::anyhow!("{}", e))?
        };

        applied
            .into_iter()
            .map(|(path, kind)| format!("{} ({})", path, kind))
            .collect()
    } else {
        // Legacy MCP-based goal: use FsConnector.
        if selective_review {
            anyhow::bail!(
                "Selective review is not supported for MCP-based goals (only overlay-based goals)"
            );
        }
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

    // Submit workflow integration (git or other adapters).
    if git_commit {
        use ta_submit::{GitAdapter, NoneAdapter, SubmitAdapter, WorkflowConfig};

        // Load workflow config if it exists.
        let workflow_config_path = target_dir.join(".ta/workflow.toml");
        let workflow_config = WorkflowConfig::load_or_default(&workflow_config_path);

        // Select adapter based on config.
        // Default to "git" if in a git repo and no config exists (backwards compatibility).
        let is_git_repo = target_dir.join(".git").exists();
        let adapter_name = if workflow_config.submit.adapter == "none" && is_git_repo {
            "git"
        } else {
            &workflow_config.submit.adapter
        };

        let adapter: Box<dyn SubmitAdapter> = match adapter_name {
            "git" => Box::new(GitAdapter::new(&target_dir)),
            _ => Box::new(NoneAdapter::new()),
        };

        println!("\nUsing submit adapter: {}", adapter.name());

        // Prepare (create branch if needed).
        if let Err(e) = adapter.prepare(goal, &workflow_config.submit) {
            eprintln!("Warning: adapter prepare failed: {}", e);
        }

        // Commit changes.
        println!("Committing changes...");
        let commit_msg = format!(
            "{}\n\nApplied via Trusted Autonomy PR package {}",
            pkg.summary.what_changed, package_id
        );

        match adapter.commit(goal, &pkg, &commit_msg) {
            Ok(result) => {
                println!("✓ {}", result.message);
            }
            Err(e) => {
                eprintln!("Commit failed: {}", e);
                // Continue anyway if this is a "none" adapter
                if adapter.name() != "none" {
                    anyhow::bail!("Failed to commit changes: {}", e);
                }
            }
        }

        // Push to remote if requested.
        if git_push {
            println!("Pushing to remote...");
            match adapter.push(goal) {
                Ok(result) => {
                    println!("✓ {}", result.message);
                }
                Err(e) => {
                    if adapter.name() != "none" {
                        anyhow::bail!("Failed to push: {}", e);
                    }
                }
            }
        }

        // Open review (PR) if requested.
        if git_review {
            println!("Creating pull request...");
            match adapter.open_review(goal, &pkg) {
                Ok(result) => {
                    println!("✓ {}", result.message);
                    if !result.review_url.starts_with("none://") {
                        println!("  PR URL: {}", result.review_url);
                    }
                }
                Err(e) => {
                    eprintln!("Warning: PR creation failed: {}", e);
                    eprintln!("  You can manually create a PR from the pushed branch.");
                }
            }
        }
    }

    // Transition goal to Applied → update package status.
    let _ = goal_store.transition(goal.goal_run_id, GoalRunState::Applied);
    pkg.status = PRStatus::Applied {
        applied_at: Utc::now(),
    };
    save_package(config, &pkg)?;

    // If the goal has a plan_phase, mark it done in PLAN.md.
    if let Some(ref phase) = goal.plan_phase {
        let plan_path = target_dir.join("PLAN.md");
        if plan_path.exists() {
            let content = std::fs::read_to_string(&plan_path)?;
            let updated =
                super::plan::update_phase_status(&content, phase, super::plan::PlanStatus::Done);
            std::fs::write(&plan_path, updated)?;
            println!("Updated PLAN.md: Phase {} -> done", phase);
        }
    }

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

pub fn load_package(config: &GatewayConfig, package_id: Uuid) -> anyhow::Result<PRPackage> {
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
                phase: None,
                follow_up: None,
                objective_file: None,
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
        build_package(&config, &goal_id, "Test changes", false).unwrap();

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
                phase: None,
                follow_up: None,
                objective_file: None,
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
        build_package(&config, &goal_id, "Test apply changes", false).unwrap();

        // Approve the PR.
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();
        approve_package(&config, &pkg_id, "tester").unwrap();

        // Apply (no git).
        apply_package(
            &config,
            &pkg_id,
            None,
            false,
            false,
            false,
            SelectiveReviewPatterns::default(),
        )
        .unwrap();

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
                phase: None,
                follow_up: None,
                objective_file: None,
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
        build_package(&config, &goal_id, "Modified README", false).unwrap();
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();
        approve_package(&config, &pkg_id, "tester").unwrap();
        apply_package(
            &config,
            &pkg_id,
            None,
            true,
            false,
            false,
            SelectiveReviewPatterns::default(),
        )
        .unwrap();

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
    fn build_pr_enriches_from_change_summary() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Original\n").unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n").unwrap();

        let config = GatewayConfig::for_project(project.path());
        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();

        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Summary test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test change_summary ingestion".to_string(),
                agent: "test-agent".to_string(),
                phase: None,
                follow_up: None,
                objective_file: None,
            },
            &config,
        )
        .unwrap();

        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        // Make changes in staging.
        std::fs::write(goal.workspace_path.join("README.md"), "# Updated README\n").unwrap();
        std::fs::write(
            goal.workspace_path.join("src/main.rs"),
            "fn main() { println!(\"hello\"); }\n",
        )
        .unwrap();

        // Write a change_summary.json in staging .ta/.
        std::fs::create_dir_all(goal.workspace_path.join(".ta")).unwrap();
        std::fs::write(
            goal.workspace_path.join(".ta/change_summary.json"),
            r#"{
                "summary": "Updated README and main entry point",
                "changes": [
                    {
                        "path": "README.md",
                        "action": "modified",
                        "why": "Updated project description",
                        "independent": true,
                        "depends_on": [],
                        "depended_by": []
                    },
                    {
                        "path": "src/main.rs",
                        "action": "modified",
                        "why": "Added hello world output",
                        "independent": false,
                        "depends_on": [],
                        "depended_by": ["README.md"]
                    }
                ],
                "dependency_notes": "main.rs change is referenced in README"
            }"#,
        )
        .unwrap();

        // Build PR with default summary (triggers agent summary usage).
        build_package(&config, &goal_id, "Changes from agent work", false).unwrap();

        let packages = load_all_packages(&config).unwrap();
        let pkg = &packages[0];

        // Summary should come from change_summary.json.
        assert_eq!(
            pkg.summary.what_changed,
            "Updated README and main entry point"
        );

        // Dependency notes in open_questions.
        assert!(pkg
            .summary
            .open_questions
            .contains(&"main.rs change is referenced in README".to_string()));

        // Artifacts should have rationale populated.
        let readme_artifact = pkg
            .changes
            .artifacts
            .iter()
            .find(|a| a.resource_uri.contains("README.md"))
            .unwrap();
        assert_eq!(
            readme_artifact.rationale.as_deref(),
            Some("Updated project description")
        );
        assert!(readme_artifact.dependencies.is_empty());

        let main_artifact = pkg
            .changes
            .artifacts
            .iter()
            .find(|a| a.resource_uri.contains("main.rs"))
            .unwrap();
        assert_eq!(
            main_artifact.rationale.as_deref(),
            Some("Added hello world output")
        );
        assert_eq!(main_artifact.dependencies.len(), 1);
        assert_eq!(
            main_artifact.dependencies[0].target_uri,
            "fs://workspace/README.md"
        );
        assert_eq!(
            main_artifact.dependencies[0].kind,
            DependencyKind::DependedBy
        );
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
                phase: None,
                follow_up: None,
                objective_file: None,
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal_id = goals[0].goal_run_id.to_string();

        // Build PR should fail — no changes.
        let result = build_package(&config, &goal_id, "No changes", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No changes"));
    }

    // ── Selective Review Tests ──────────────────────────────────────

    #[test]
    fn selective_apply_with_approve_pattern() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Test\n").unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();

        let config = GatewayConfig::for_project(project.path());

        // Start goal + modify files + build PR.
        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Selective test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test selective approval".to_string(),
                agent: "test-agent".to_string(),
                phase: None,
                follow_up: None,
                objective_file: None,
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        // Modify all files.
        std::fs::write(goal.workspace_path.join("README.md"), "# Updated\n").unwrap();
        std::fs::write(
            goal.workspace_path.join("src/main.rs"),
            "fn main() { println!(\"hi\"); }\n",
        )
        .unwrap();
        std::fs::write(
            goal.workspace_path.join("src/lib.rs"),
            "pub fn lib() { println!(\"lib\"); }\n",
        )
        .unwrap();

        // Build PR.
        build_package(&config, &goal_id, "Test changes", false).unwrap();
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();

        // Apply with selective approval: only approve src/**
        apply_package(
            &config,
            &pkg_id,
            None,
            false,
            false,
            false,
            SelectiveReviewPatterns {
                approve: &["src/**".to_string()],
                reject: &[],
                discuss: &[],
            },
        )
        .unwrap();

        // Verify only src files changed.
        let readme = std::fs::read_to_string(project.path().join("README.md")).unwrap();
        assert_eq!(readme, "# Test\n"); // unchanged

        let main = std::fs::read_to_string(project.path().join("src/main.rs")).unwrap();
        assert!(main.contains("println")); // changed

        let lib = std::fs::read_to_string(project.path().join("src/lib.rs")).unwrap();
        assert!(lib.contains("println")); // changed
    }

    #[test]
    fn selective_apply_with_reject_pattern() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Test\n").unwrap();
        std::fs::write(project.path().join("config.toml"), "[config]\n").unwrap();

        let config = GatewayConfig::for_project(project.path());

        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Reject test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test rejection".to_string(),
                agent: "test-agent".to_string(),
                phase: None,
                follow_up: None,
                objective_file: None,
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        std::fs::write(goal.workspace_path.join("README.md"), "# Updated\n").unwrap();
        std::fs::write(goal.workspace_path.join("config.toml"), "[config]\nfoo=1\n").unwrap();

        build_package(&config, &goal_id, "Test changes", false).unwrap();
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();

        // Approve all, then reject config.toml.
        apply_package(
            &config,
            &pkg_id,
            None,
            false,
            false,
            false,
            SelectiveReviewPatterns {
                approve: &["all".to_string()],
                reject: &["config.toml".to_string()],
                discuss: &[],
            },
        )
        .unwrap();

        // Only README should be updated.
        let readme = std::fs::read_to_string(project.path().join("README.md")).unwrap();
        assert_eq!(readme, "# Updated\n");

        let config_content = std::fs::read_to_string(project.path().join("config.toml")).unwrap();
        assert_eq!(config_content, "[config]\n"); // unchanged
    }

    #[test]
    fn selective_apply_special_value_all() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("file1.txt"), "a\n").unwrap();
        std::fs::write(project.path().join("file2.txt"), "b\n").unwrap();

        let config = GatewayConfig::for_project(project.path());

        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "All test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test all".to_string(),
                agent: "test-agent".to_string(),
                phase: None,
                follow_up: None,
                objective_file: None,
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        std::fs::write(goal.workspace_path.join("file1.txt"), "a-updated\n").unwrap();
        std::fs::write(goal.workspace_path.join("file2.txt"), "b-updated\n").unwrap();

        build_package(&config, &goal_id, "Test changes", false).unwrap();
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();

        // Use "all" to approve everything.
        apply_package(
            &config,
            &pkg_id,
            None,
            false,
            false,
            false,
            SelectiveReviewPatterns {
                approve: &["all".to_string()],
                reject: &[],
                discuss: &[],
            },
        )
        .unwrap();

        let file1 = std::fs::read_to_string(project.path().join("file1.txt")).unwrap();
        assert_eq!(file1, "a-updated\n");

        let file2 = std::fs::read_to_string(project.path().join("file2.txt")).unwrap();
        assert_eq!(file2, "b-updated\n");
    }

    #[test]
    fn selective_apply_special_value_rest() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("important.txt"), "keep\n").unwrap();
        std::fs::write(project.path().join("other.txt"), "skip\n").unwrap();

        let config = GatewayConfig::for_project(project.path());

        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Rest test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test rest".to_string(),
                agent: "test-agent".to_string(),
                phase: None,
                follow_up: None,
                objective_file: None,
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        std::fs::write(goal.workspace_path.join("important.txt"), "keep-updated\n").unwrap();
        std::fs::write(goal.workspace_path.join("other.txt"), "skip-updated\n").unwrap();

        build_package(&config, &goal_id, "Test changes", false).unwrap();
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();

        // Reject important, approve "rest".
        apply_package(
            &config,
            &pkg_id,
            None,
            false,
            false,
            false,
            SelectiveReviewPatterns {
                approve: &["rest".to_string()],
                reject: &["important.txt".to_string()],
                discuss: &[],
            },
        )
        .unwrap();

        // important should be unchanged (rejected before "rest").
        let important = std::fs::read_to_string(project.path().join("important.txt")).unwrap();
        assert_eq!(important, "keep\n");

        // other should be updated (approved by "rest").
        let other = std::fs::read_to_string(project.path().join("other.txt")).unwrap();
        assert_eq!(other, "skip-updated\n");
    }

    #[test]
    fn selective_apply_dependency_validation_fails() {
        let project = TempDir::new().unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();

        let config = GatewayConfig::for_project(project.path());

        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Dependency test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test dependencies".to_string(),
                agent: "test-agent".to_string(),
                phase: None,
                follow_up: None,
                objective_file: None,
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        std::fs::write(
            goal.workspace_path.join("src/main.rs"),
            "fn main() { lib::lib(); }\n",
        )
        .unwrap();
        std::fs::write(
            goal.workspace_path.join("src/lib.rs"),
            "pub mod lib { pub fn lib() {} }\n",
        )
        .unwrap();

        // Write a change_summary.json declaring the dependency.
        std::fs::create_dir_all(goal.workspace_path.join(".ta")).unwrap();
        std::fs::write(
            goal.workspace_path.join(".ta/change_summary.json"),
            r#"{
                "summary": "Updated main and lib",
                "changes": [
                    {
                        "path": "src/main.rs",
                        "action": "modified",
                        "why": "Call lib function",
                        "independent": false,
                        "depends_on": ["src/lib.rs"],
                        "depended_by": []
                    },
                    {
                        "path": "src/lib.rs",
                        "action": "modified",
                        "why": "Add lib module",
                        "independent": false,
                        "depends_on": [],
                        "depended_by": ["src/main.rs"]
                    }
                ]
            }"#,
        )
        .unwrap();

        build_package(&config, &goal_id, "Changes from agent work", false).unwrap();
        let packages = load_all_packages(&config).unwrap();
        let pkg_id = packages[0].package_id.to_string();

        // Try to approve main.rs but reject lib.rs — should fail dependency check.
        let result = apply_package(
            &config,
            &pkg_id,
            None,
            false,
            false,
            false,
            SelectiveReviewPatterns {
                approve: &["src/main.rs".to_string()],
                reject: &["src/lib.rs".to_string()],
                discuss: &[],
            },
        );

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("dependency conflict"));
    }

    #[test]
    fn build_pr_excludes_target_dir() {
        // Set up a source project with a target/ directory (simulates Rust build artifacts).
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Test\n").unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n").unwrap();

        // Create a target/ directory with build artifacts in source.
        std::fs::create_dir_all(project.path().join("target/debug/incremental")).unwrap();
        std::fs::write(
            project.path().join("target/debug/incremental/artifact.o"),
            "binary-data",
        )
        .unwrap();

        let config = GatewayConfig::for_project(project.path());

        super::super::goal::execute(
            &super::super::goal::GoalCommands::Start {
                title: "Exclude test".to_string(),
                source: Some(project.path().to_path_buf()),
                objective: "Test exclusion".to_string(),
                agent: "test-agent".to_string(),
                phase: None,
                follow_up: None,
                objective_file: None,
            },
            &config,
        )
        .unwrap();

        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        let goal = &goals[0];
        let goal_id = goal.goal_run_id.to_string();

        // Verify target/ was NOT copied to staging.
        assert!(!goal.workspace_path.join("target").exists());

        // Modify a real source file.
        std::fs::write(goal.workspace_path.join("README.md"), "# Updated\n").unwrap();

        // Also create target/ in staging to simulate agent building in staging.
        std::fs::create_dir_all(goal.workspace_path.join("target/debug/incremental")).unwrap();
        std::fs::write(
            goal.workspace_path
                .join("target/debug/incremental/ta_workspace-123"),
            "build-artifact",
        )
        .unwrap();

        // Build PR — target/ should be excluded from artifacts.
        build_package(&config, &goal_id, "Test changes", false).unwrap();

        let packages = load_all_packages(&config).unwrap();
        let pkg = &packages[0];

        // Only the real source change should be in artifacts.
        assert_eq!(pkg.changes.artifacts.len(), 1);
        assert!(pkg.changes.artifacts[0].resource_uri.contains("README.md"));

        // No target/ files should appear.
        for artifact in &pkg.changes.artifacts {
            assert!(
                !artifact.resource_uri.contains("target/"),
                "target/ file should be excluded: {}",
                artifact.resource_uri
            );
        }
    }
}
