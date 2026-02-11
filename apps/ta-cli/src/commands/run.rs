// run.rs — Orchestrate a full goal lifecycle: start → agent → PR build.
//
// `ta run claude-code "Fix the auth bug"` is a convenience wrapper that:
// 1. Creates a goal with an overlay workspace
// 2. Prints the staging path for the agent
// 3. Optionally launches the agent (claude, etc.)
// 4. When the agent exits, builds a PR package from the diff
//
// The user then reviews/approves/applies via `ta pr` commands.

use std::path::PathBuf;

use clap::Subcommand;
use ta_goal::GoalRunStore;
use ta_mcp_gateway::GatewayConfig;

#[derive(Subcommand)]
pub enum RunCommands {
    /// Run Claude Code in a TA-mediated staging workspace.
    ClaudeCode {
        /// Goal title describing what to accomplish.
        title: String,
        /// Source directory (defaults to project root).
        #[arg(long)]
        source: Option<PathBuf>,
        /// Detailed objective.
        #[arg(long, default_value = "")]
        objective: String,
        /// Don't launch Claude Code — just set up the workspace.
        #[arg(long)]
        no_launch: bool,
    },
}

pub fn execute(cmd: &RunCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match cmd {
        RunCommands::ClaudeCode {
            title,
            source,
            objective,
            no_launch,
        } => run_claude_code(config, title, source.as_deref(), objective, *no_launch),
    }
}

fn run_claude_code(
    config: &GatewayConfig,
    title: &str,
    source: Option<&std::path::Path>,
    objective: &str,
    no_launch: bool,
) -> anyhow::Result<()> {
    // 1. Start the goal (creates overlay workspace).
    let goal_store = GoalRunStore::new(&config.goals_dir)?;

    super::goal::execute(
        &super::goal::GoalCommands::Start {
            title: title.to_string(),
            source: source.map(|p| p.to_path_buf()),
            objective: objective.to_string(),
            agent: "claude-code".to_string(),
        },
        config,
    )?;

    // Get the goal we just created (most recent).
    let goals = goal_store.list()?;
    let goal = goals
        .first()
        .ok_or_else(|| anyhow::anyhow!("Failed to find created goal"))?;
    let goal_id = goal.goal_run_id.to_string();
    let staging_path = goal.workspace_path.clone();

    // 2. Inject a CLAUDE.md into the staging workspace.
    inject_claude_md(&staging_path, title, &goal_id)?;

    if no_launch {
        println!("\nWorkspace ready. To use manually:");
        println!("  cd {}", staging_path.display());
        println!("  claude");
        println!();
        println!("When done, build the PR:");
        println!("  ta pr build {}", goal_id);
        return Ok(());
    }

    // 3. Launch Claude Code in the staging directory.
    println!("\nLaunching Claude Code in staging workspace...");
    println!("  Working dir: {}", staging_path.display());
    println!();

    let status = std::process::Command::new("claude")
        .current_dir(&staging_path)
        .status();

    match status {
        Ok(exit) => {
            if exit.success() {
                println!("\nClaude Code exited. Building PR package...");
            } else {
                println!(
                    "\nClaude Code exited with status {}. Building PR package anyway...",
                    exit
                );
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                println!("\n'claude' command not found. To use manually:");
                println!("  cd {}", staging_path.display());
                println!("  claude  (or your preferred agent)");
                println!();
                println!("When done, build the PR:");
                println!("  ta pr build {}", goal_id);
                return Ok(());
            }
            return Err(anyhow::anyhow!("Failed to launch Claude Code: {}", e));
        }
    }

    // 4. Build PR package from the diff.
    super::pr::execute(
        &super::pr::PrCommands::Build {
            goal_id: goal_id.clone(),
            summary: format!("Changes from goal: {}", title),
        },
        config,
    )?;

    println!("\nNext steps:");
    println!("  ta pr list");
    println!("  ta pr view <package-id>");
    println!("  ta pr approve <package-id>");
    println!("  ta pr apply <package-id> --git-commit");

    Ok(())
}

/// Inject a CLAUDE.md file into the staging workspace to orient Claude Code.
fn inject_claude_md(
    staging_path: &std::path::Path,
    title: &str,
    goal_id: &str,
) -> anyhow::Result<()> {
    let existing_claude_md = staging_path.join("CLAUDE.md");
    let existing_content = if existing_claude_md.exists() {
        std::fs::read_to_string(&existing_claude_md)?
    } else {
        String::new()
    };

    let injected = format!(
        r#"# Trusted Autonomy — Mediated Goal

You are working on a TA-mediated goal in a staging workspace.

**Goal:** {}
**Goal ID:** {}

## How this works

- This directory is a copy of the original project
- Work normally — Read, Write, Edit, Bash all work as expected
- When you're done, just exit. TA will diff your changes and create a PR package
- The human reviewer will see exactly what you changed and why

## Important

- Do NOT modify files outside this directory
- All your changes will be captured as a PR for review

---

{}
"#,
        title, goal_id, existing_content
    );

    std::fs::write(&existing_claude_md, injected)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn run_creates_goal_and_injects_claude_md() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("README.md"), "# Test\n").unwrap();
        std::fs::write(
            project.path().join("CLAUDE.md"),
            "# Existing project instructions\n",
        )
        .unwrap();

        let config = GatewayConfig::for_project(project.path());

        // Run with --no-launch to avoid actually starting claude.
        run_claude_code(
            &config,
            "Test goal",
            Some(project.path()),
            "Test objective",
            true,
        )
        .unwrap();

        // Verify goal was created.
        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        assert_eq!(goals.len(), 1);

        // Verify CLAUDE.md was injected.
        let claude_md = std::fs::read_to_string(goals[0].workspace_path.join("CLAUDE.md")).unwrap();
        assert!(claude_md.contains("Trusted Autonomy"));
        assert!(claude_md.contains("Test goal"));
        assert!(claude_md.contains("Existing project instructions"));
    }
}
