// goal.rs — Goal subcommands: start, list, status.

use std::path::PathBuf;

use clap::Subcommand;
use ta_goal::{GoalRunState, GoalRunStore};
use ta_mcp_gateway::GatewayConfig;
use ta_workspace::{ExcludePatterns, OverlayWorkspace};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum GoalCommands {
    /// Start a new goal run with an overlay workspace.
    Start {
        /// Goal title (e.g., "Fix authentication bug").
        title: String,
        /// Source directory to overlay (defaults to project root).
        #[arg(long)]
        source: Option<PathBuf>,
        /// Detailed objective for the goal.
        #[arg(long, default_value = "")]
        objective: String,
        /// Agent identity.
        #[arg(long, default_value = "claude-code")]
        agent: String,
        /// Plan phase this goal implements (e.g., "4b").
        #[arg(long)]
        phase: Option<String>,
        /// Follow up on a previous goal (ID prefix or omit for latest).
        #[arg(long)]
        follow_up: Option<Option<String>>,
        /// Read objective from a file instead of --objective.
        #[arg(long)]
        objective_file: Option<PathBuf>,
    },
    /// List all goal runs.
    List {
        /// Filter by state (e.g., "running", "pr_ready", "completed").
        #[arg(long)]
        state: Option<String>,
    },
    /// Show details for a specific goal run.
    Status {
        /// Goal run ID.
        id: String,
    },
    /// Delete a goal run and its staging directory.
    Delete {
        /// Goal run ID.
        id: String,
    },
}

/// Find a parent goal by ID prefix, or return the latest goal if no prefix given.
fn find_parent_goal(store: &GoalRunStore, id_prefix: Option<&str>) -> anyhow::Result<Uuid> {
    match id_prefix {
        Some(prefix) => {
            // Match by ID prefix (first N characters).
            let all_goals = store.list()?;
            let matches: Vec<_> = all_goals
                .iter()
                .filter(|g| g.goal_run_id.to_string().starts_with(prefix))
                .collect();

            match matches.len() {
                0 => anyhow::bail!("No goal found matching prefix '{}'", prefix),
                1 => Ok(matches[0].goal_run_id),
                _ => {
                    anyhow::bail!(
                        "Ambiguous prefix '{}' matches {} goals. Use a longer prefix.",
                        prefix,
                        matches.len()
                    )
                }
            }
        }
        None => {
            // Find the most recent goal (prefer unapplied, fall back to latest applied).
            let all_goals = store.list()?;
            if all_goals.is_empty() {
                anyhow::bail!(
                    "No previous goals found. Cannot use --follow-up without an existing goal."
                );
            }

            // Sort by updated_at descending.
            let mut sorted = all_goals.clone();
            sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            // Prefer goals that haven't been applied yet.
            let unapplied = sorted
                .iter()
                .find(|g| !matches!(g.state, GoalRunState::Applied | GoalRunState::Completed));

            if let Some(goal) = unapplied {
                Ok(goal.goal_run_id)
            } else {
                // Fall back to the most recent goal.
                Ok(sorted[0].goal_run_id)
            }
        }
    }
}

pub fn execute(cmd: &GoalCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    let store = GoalRunStore::new(&config.goals_dir)?;

    match cmd {
        GoalCommands::Start {
            title,
            source,
            objective,
            agent,
            phase,
            follow_up,
            objective_file,
        } => start_goal(
            config,
            &store,
            title,
            source.as_deref(),
            objective,
            agent,
            phase.as_deref(),
            follow_up.as_ref(),
            objective_file.as_deref(),
        ),
        GoalCommands::List { state } => list_goals(&store, state.as_deref()),
        GoalCommands::Status { id } => show_status(&store, id),
        GoalCommands::Delete { id } => delete_goal(&store, id),
    }
}

#[allow(clippy::too_many_arguments)]
fn start_goal(
    config: &GatewayConfig,
    store: &GoalRunStore,
    title: &str,
    source: Option<&std::path::Path>,
    objective: &str,
    agent: &str,
    phase: Option<&str>,
    follow_up: Option<&Option<String>>,
    objective_file: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    // Resolve objective from file if specified.
    let final_objective = if let Some(obj_file) = objective_file {
        std::fs::read_to_string(obj_file)?
    } else if objective.is_empty() {
        title.to_string()
    } else {
        objective.to_string()
    };

    // Find parent goal if --follow-up is specified.
    let parent_goal_id = if let Some(follow_up_arg) = follow_up {
        Some(find_parent_goal(store, follow_up_arg.as_deref())?)
    } else {
        None
    };

    let source_dir = match source {
        Some(p) => p.canonicalize()?,
        None => config.workspace_root.clone(),
    };

    // Create the GoalRun first to get the ID.
    let mut goal = ta_goal::GoalRun::new(
        title,
        &final_objective,
        agent,
        PathBuf::new(), // placeholder — set after overlay creation
        config.store_dir.join("placeholder"), // placeholder — set after overlay creation
    );

    // Set parent goal ID if this is a follow-up.
    goal.parent_goal_id = parent_goal_id;

    let goal_id = goal.goal_run_id.to_string();

    // Create overlay workspace: copy source → staging.
    // V1 TEMPORARY: Load exclude patterns from .taignore or defaults.
    let excludes = ExcludePatterns::load(&source_dir);
    let overlay = OverlayWorkspace::create(&goal_id, &source_dir, &config.staging_dir, excludes)?;

    // v0.2.1: Capture source snapshot for conflict detection.
    let snapshot_json = overlay
        .snapshot()
        .and_then(|snap| serde_json::to_value(snap).ok());

    // Update goal with actual paths.
    goal.workspace_path = overlay.staging_dir().to_path_buf();
    goal.store_path = config.store_dir.join(&goal_id);
    goal.source_dir = Some(source_dir);
    goal.plan_phase = phase.map(|p| p.to_string());
    goal.source_snapshot = snapshot_json;

    // Transition: Created → Configured → Running.
    goal.transition(GoalRunState::Configured)?;
    goal.transition(GoalRunState::Running)?;

    store.save(&goal)?;

    println!("Goal started: {}", goal.goal_run_id);
    println!("  Title:   {}", goal.title);
    println!("  Staging: {}", overlay.staging_dir().display());
    println!();
    println!("Agent workspace ready. To enter:");
    println!("  cd {}", overlay.staging_dir().display());

    Ok(())
}

fn list_goals(store: &GoalRunStore, state: Option<&str>) -> anyhow::Result<()> {
    let goals = if let Some(state_filter) = state {
        store.list_by_state(state_filter)?
    } else {
        store.list()?
    };

    if goals.is_empty() {
        println!("No goal runs found.");
        return Ok(());
    }

    println!(
        "{:<38} {:<30} {:<14} {:<12}",
        "ID", "TITLE", "STATE", "AGENT"
    );
    println!("{}", "-".repeat(94));

    for g in &goals {
        let title_with_chain = if g.is_macro {
            format!("[M] {}", truncate(&g.title, 24))
        } else if let Some(ref macro_id) = g.parent_macro_id {
            format!(
                "  └ {} (← {})",
                truncate(&g.title, 16),
                &macro_id.to_string()[..8]
            )
        } else if let Some(parent_id) = g.parent_goal_id {
            format!(
                "{} (→ {})",
                truncate(&g.title, 20),
                &parent_id.to_string()[..8]
            )
        } else {
            truncate(&g.title, 28)
        };

        println!(
            "{:<38} {:<30} {:<14} {:<12}",
            g.goal_run_id,
            title_with_chain,
            g.state.to_string(),
            g.agent_id,
        );
    }
    println!("\n{} goal(s) total.", goals.len());

    Ok(())
}

fn show_status(store: &GoalRunStore, id: &str) -> anyhow::Result<()> {
    let goal_run_id = Uuid::parse_str(id)?;
    match store.get(goal_run_id)? {
        Some(g) => {
            println!("Goal Run: {}", g.goal_run_id);
            println!("Title:    {}", g.title);
            println!("Objective: {}", g.objective);
            println!("State:    {}", g.state);
            println!("Agent:    {}", g.agent_id);
            println!("Created:  {}", g.created_at.to_rfc3339());
            println!("Updated:  {}", g.updated_at.to_rfc3339());
            if let Some(ref src) = g.source_dir {
                println!("Source:   {}", src.display());
            }
            if let Some(ref phase) = g.plan_phase {
                println!("Phase:    {}", phase);
            }
            if let Some(parent_id) = g.parent_goal_id {
                println!("Parent:   {} (follow-up)", parent_id);
            }
            if let Some(ref macro_id) = g.parent_macro_id {
                println!("Macro:    {} (sub-goal of macro)", macro_id);
            }
            if g.is_macro {
                println!("Mode:     macro goal (inner-loop iteration)");
            }
            println!("Staging:  {}", g.workspace_path.display());
            if let Some(pr_id) = g.pr_package_id {
                println!("Draft:    {}", pr_id);
            }

            // Show sub-goal tree for macro goals.
            if g.is_macro && !g.sub_goal_ids.is_empty() {
                println!("\nSub-goals ({}):", g.sub_goal_ids.len());
                for sub_id in &g.sub_goal_ids {
                    match store.get(*sub_id)? {
                        Some(sg) => {
                            let draft_status = sg
                                .pr_package_id
                                .map(|id| format!(" [draft: {}]", &id.to_string()[..8]))
                                .unwrap_or_default();
                            println!(
                                "  {} {} [{}]{}",
                                &sub_id.to_string()[..8],
                                truncate(&sg.title, 40),
                                sg.state,
                                draft_status,
                            );
                        }
                        None => {
                            println!("  {} (not found)", sub_id);
                        }
                    }
                }
            }
        }
        None => {
            eprintln!("Goal run not found: {}", id);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn delete_goal(store: &GoalRunStore, id: &str) -> anyhow::Result<()> {
    let goal_run_id = uuid::Uuid::parse_str(id)?;
    let goal = store.get(goal_run_id)?;

    match goal {
        Some(g) => {
            // Remove the staging directory if it exists.
            let workspace = &g.workspace_path;
            if workspace.exists() {
                std::fs::remove_dir_all(workspace)?;
                println!("Removed staging directory: {}", workspace.display());
            }

            // Remove goal metadata from the store.
            store.delete(goal_run_id)?;
            println!("Deleted goal: {} ({})", g.title, goal_run_id);
        }
        None => {
            eprintln!("Goal run not found: {}", id);
            std::process::exit(1);
        }
    }

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
    fn start_goal_creates_overlay_and_goal_run() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Test\n").unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n").unwrap();

        let config = GatewayConfig::for_project(project.path());
        let store = GoalRunStore::new(&config.goals_dir).unwrap();

        start_goal(
            &config,
            &store,
            "Test goal",
            Some(project.path()),
            "Test objective",
            "test-agent",
            None,
            None,
            None,
        )
        .unwrap();

        // Verify goal was created.
        let goals = store.list().unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].title, "Test goal");
        assert_eq!(goals[0].state, GoalRunState::Running);
        assert!(goals[0].source_dir.is_some());

        // Verify staging workspace was created.
        assert!(goals[0].workspace_path.exists());
        assert!(goals[0].workspace_path.join("README.md").exists());
        assert!(goals[0].workspace_path.join("src/main.rs").exists());
    }

    #[test]
    fn delete_goal_removes_metadata_and_staging() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Test\n").unwrap();

        let config = GatewayConfig::for_project(project.path());
        let store = GoalRunStore::new(&config.goals_dir).unwrap();

        // Create a goal to delete.
        start_goal(
            &config,
            &store,
            "Doomed goal",
            Some(project.path()),
            "Will be deleted",
            "test-agent",
            None,
            None,
            None,
        )
        .unwrap();

        let goals = store.list().unwrap();
        assert_eq!(goals.len(), 1);
        let goal_id = goals[0].goal_run_id;
        let staging_path = goals[0].workspace_path.clone();
        assert!(staging_path.exists());

        // Delete the goal.
        delete_goal(&store, &goal_id.to_string()).unwrap();

        // Verify metadata is removed.
        assert!(store.get(goal_id).unwrap().is_none());

        // Verify staging directory is removed.
        assert!(!staging_path.exists());
    }
}
