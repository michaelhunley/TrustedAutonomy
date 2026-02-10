// goal.rs — Goal subcommands: list, status.

use clap::Subcommand;
use ta_goal::GoalRunStore;
use ta_mcp_gateway::GatewayConfig;

#[derive(Subcommand)]
pub enum GoalCommands {
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
}

pub fn execute(cmd: &GoalCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    let store = GoalRunStore::new(&config.goals_dir)?;

    match cmd {
        GoalCommands::List { state } => {
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
        }

        GoalCommands::Status { id } => {
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
                    if let Some(pr_id) = g.pr_package_id {
                        println!("PR Pkg:   {}", pr_id);
                    }
                }
                None => {
                    eprintln!("Goal run not found: {}", id);
                    std::process::exit(1);
                }
            }
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
