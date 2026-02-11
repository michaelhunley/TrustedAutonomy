//! # ta-cli
//!
//! Command-line interface for Trusted Autonomy.
//!
//! Provides human review and approval workflow for agent-staged changes:
//! - `ta goal list/status` — inspect active goal runs
//! - `ta pr list/view/approve/deny/apply` — review and manage PR packages
//! - `ta audit verify/tail` — inspect the tamper-evident audit trail
//! - `ta adapter list/install` — manage agent adapter integrations
//! - `ta serve` — start MCP server on stdio

mod commands;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ta_mcp_gateway::GatewayConfig;

/// Trusted Autonomy CLI — review and approve agent changes.
#[derive(Parser)]
#[command(name = "ta", version, about)]
struct Cli {
    /// Project root directory (defaults to current directory).
    #[arg(long, default_value = ".")]
    project_root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage goal runs.
    Goal {
        #[command(subcommand)]
        command: commands::goal::GoalCommands,
    },
    /// Review and manage PR packages.
    Pr {
        #[command(subcommand)]
        command: commands::pr::PrCommands,
    },
    /// Inspect the audit trail.
    Audit {
        #[command(subcommand)]
        command: commands::audit::AuditCommands,
    },
    /// Run an agent in a TA-mediated staging workspace.
    Run {
        /// Goal title describing what to accomplish.
        title: String,
        /// Agent system to use (claude-code, codex, etc.).
        #[arg(long, default_value = "claude-code")]
        agent: String,
        /// Source directory to overlay (defaults to project root).
        #[arg(long)]
        source: Option<PathBuf>,
        /// Detailed objective for the goal.
        #[arg(long, default_value = "")]
        objective: String,
        /// Plan phase this goal implements (e.g., "4b").
        #[arg(long)]
        phase: Option<String>,
        /// Don't launch the agent — just set up the workspace.
        #[arg(long)]
        no_launch: bool,
    },
    /// View and track the project development plan.
    Plan {
        #[command(subcommand)]
        command: commands::plan::PlanCommands,
    },
    /// Manage agent adapter integrations.
    Adapter {
        #[command(subcommand)]
        command: commands::adapter::AdapterCommands,
    },
    /// Start the MCP server on stdio.
    Serve,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let project_root = cli.project_root.canonicalize().unwrap_or(cli.project_root);
    let config = GatewayConfig::for_project(&project_root);

    match &cli.command {
        Commands::Goal { command } => commands::goal::execute(command, &config),
        Commands::Pr { command } => commands::pr::execute(command, &config),
        Commands::Audit { command } => commands::audit::execute(command, &config),
        Commands::Run {
            title,
            agent,
            source,
            objective,
            phase,
            no_launch,
        } => commands::run::execute(
            &config,
            title,
            agent,
            source.as_deref(),
            objective,
            phase.as_deref(),
            *no_launch,
        ),
        Commands::Plan { command } => commands::plan::execute(command, &config),
        Commands::Adapter { command } => commands::adapter::execute(command, &project_root),
        Commands::Serve => commands::serve::execute(&project_root),
    }
}
