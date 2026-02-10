// pr.rs — PR package subcommands: list, view, approve, deny, apply.

use std::fs;

use chrono::Utc;
use clap::Subcommand;
use ta_changeset::pr_package::{PRPackage, PRStatus};
use ta_connector_fs::FsConnector;
use ta_goal::{GoalRunState, GoalRunStore};
use ta_mcp_gateway::GatewayConfig;
use ta_workspace::{JsonFileStore, StagingWorkspace};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum PrCommands {
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
    },
}

pub fn execute(cmd: &PrCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match cmd {
        PrCommands::List { goal } => list_packages(config, goal.as_deref()),
        PrCommands::View { id } => view_package(config, id),
        PrCommands::Approve { id, reviewer } => approve_package(config, id, reviewer),
        PrCommands::Deny {
            id,
            reason,
            reviewer,
        } => deny_package(config, id, reason, reviewer),
        PrCommands::Apply { id, target } => apply_package(config, id, target.as_deref()),
    }
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

fn apply_package(config: &GatewayConfig, id: &str, target: Option<&str>) -> anyhow::Result<()> {
    let package_id = Uuid::parse_str(id)?;
    let pkg = load_package(config, package_id)?;

    if !matches!(pkg.status, PRStatus::Approved { .. }) {
        anyhow::bail!(
            "Cannot apply package in {:?} state (must be Approved)",
            pkg.status
        );
    }

    let target_dir = match target {
        Some(t) => std::path::PathBuf::from(t),
        None => config.workspace_root.clone(),
    };

    // Find the goal for this package to get the staging workspace.
    let goal_store = GoalRunStore::new(&config.goals_dir)?;
    let goals = goal_store.list()?;
    let goal = goals
        .iter()
        .find(|g| g.pr_package_id == Some(package_id))
        .ok_or_else(|| anyhow::anyhow!("No goal found for PR package {}", package_id))?;

    let staging = StagingWorkspace::new(goal.goal_run_id.to_string(), &config.staging_dir)?;
    let store = JsonFileStore::new(config.store_dir.join(goal.goal_run_id.to_string()))?;
    let mut connector =
        FsConnector::new(goal.goal_run_id.to_string(), staging, store, &goal.agent_id);

    let applied = connector.apply(&target_dir)?;

    // Transition goal to Applied.
    let _ = goal_store.transition(goal.goal_run_id, GoalRunState::Applied);

    println!(
        "Applied {} file(s) to {}",
        applied.len(),
        target_dir.display()
    );
    for file in &applied {
        println!("  {}", file);
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
