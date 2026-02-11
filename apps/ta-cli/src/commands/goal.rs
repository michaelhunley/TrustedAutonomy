// goal.rs — Goal subcommands: start, list, status.

use std::path::PathBuf;

use clap::Subcommand;
use ta_goal::{GoalRunState, GoalRunStore};
use ta_mcp_gateway::GatewayConfig;
use ta_workspace::{ExcludePatterns, OverlayWorkspace};

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

pub fn execute(cmd: &GoalCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    let store = GoalRunStore::new(&config.goals_dir)?;

    match cmd {
        GoalCommands::Start {
            title,
            source,
            objective,
            agent,
            phase,
        } => start_goal(
            config,
            &store,
            title,
            source.as_deref(),
            objective,
            agent,
            phase.as_deref(),
        ),
        GoalCommands::List { state } => list_goals(&store, state.as_deref()),
        GoalCommands::Status { id } => show_status(&store, id),
        GoalCommands::Delete { id } => delete_goal(&store, id),
    }
}

fn start_goal(
    config: &GatewayConfig,
    store: &GoalRunStore,
    title: &str,
    source: Option<&std::path::Path>,
    objective: &str,
    agent: &str,
    phase: Option<&str>,
) -> anyhow::Result<()> {
    let source_dir = match source {
        Some(p) => p.canonicalize()?,
        None => config.workspace_root.clone(),
    };

    // Create the GoalRun first to get the ID.
    let mut goal = ta_goal::GoalRun::new(
        title,
        if objective.is_empty() {
            title
        } else {
            objective
        },
        agent,
        PathBuf::new(), // placeholder — set after overlay creation
        config.store_dir.join("placeholder"), // placeholder — set after overlay creation
    );

    let goal_id = goal.goal_run_id.to_string();

    // Create overlay workspace: copy source → staging.
    // V1 TEMPORARY: Load exclude patterns from .taignore or defaults.
    let excludes = ExcludePatterns::load(&source_dir);
    let overlay = OverlayWorkspace::create(&goal_id, &source_dir, &config.staging_dir, excludes)?;

    // Update goal with actual paths.
    goal.workspace_path = overlay.staging_dir().to_path_buf();
    goal.store_path = config.store_dir.join(&goal_id);
    goal.source_dir = Some(source_dir);
    goal.plan_phase = phase.map(|p| p.to_string());

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
        println!(
            "{:<38} {:<30} {:<14} {:<12}",
            g.goal_run_id,
            truncate(&g.title, 28),
            g.state.to_string(),
            g.agent_id,
        );
    }
    println!("\n{} goal(s) total.", goals.len());

    Ok(())
}

fn show_status(store: &GoalRunStore, id: &str) -> anyhow::Result<()> {
    let goal_run_id = uuid::Uuid::parse_str(id)?;
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
            println!("Staging:  {}", g.workspace_path.display());
            if let Some(pr_id) = g.pr_package_id {
                println!("PR Pkg:   {}", pr_id);
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
