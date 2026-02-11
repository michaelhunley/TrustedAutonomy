// run.rs — Orchestrate a full goal lifecycle: start → agent → PR build.
//
// `ta run "Fix the auth bug" --source .` is a convenience wrapper that:
// 1. Creates a goal with an overlay workspace
// 2. Injects context (e.g., CLAUDE.md) for the agent
// 3. Launches the agent with the goal as its initial prompt
// 4. When the agent exits, restores injected files and builds a PR package
//
// The user then reviews/approves/applies via `ta pr` commands.

use std::path::Path;

use ta_goal::GoalRunStore;
use ta_mcp_gateway::GatewayConfig;

// ── Per-agent launch configuration ──────────────────────────────

/// Built-in agent launch descriptor.
/// Describes how to invoke an agent and pass the goal prompt.
struct AgentLaunchConfig {
    /// The command to execute (e.g., "claude", "codex").
    command: String,
    /// Arguments to pass. `{prompt}` is replaced with the goal text.
    args_template: &'static [&'static str],
    /// Whether this agent reads CLAUDE.md for context injection.
    injects_context_file: bool,
}

/// Look up the built-in launch config for an agent.
fn agent_launch_config(agent_id: &str) -> AgentLaunchConfig {
    match agent_id {
        "claude-code" => AgentLaunchConfig {
            command: "claude".to_string(),
            args_template: &["--dangerously-skip-permissions", "{prompt}"],
            injects_context_file: true,
        },
        "codex" => AgentLaunchConfig {
            command: "codex".to_string(),
            args_template: &["--approval-mode", "full-auto", "{prompt}"],
            injects_context_file: false,
        },
        _ => AgentLaunchConfig {
            command: agent_id.to_string(),
            args_template: &[],
            injects_context_file: false,
        },
    }
}

// ── Public API ──────────────────────────────────────────────────

pub fn execute(
    config: &GatewayConfig,
    title: &str,
    agent: &str,
    source: Option<&Path>,
    objective: &str,
    no_launch: bool,
) -> anyhow::Result<()> {
    let agent_config = agent_launch_config(agent);

    // 1. Start the goal (creates overlay workspace).
    let goal_store = GoalRunStore::new(&config.goals_dir)?;

    super::goal::execute(
        &super::goal::GoalCommands::Start {
            title: title.to_string(),
            source: source.map(|p| p.to_path_buf()),
            objective: objective.to_string(),
            agent: agent.to_string(),
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

    // 2. Inject context file (e.g., CLAUDE.md) if the agent supports it.
    if agent_config.injects_context_file {
        inject_claude_md(&staging_path, title, &goal_id)?;
    }

    // Build the prompt string.
    let prompt = if objective.is_empty() {
        format!("Implement: {}", title)
    } else {
        format!("{}\n\nObjective: {}", title, objective)
    };

    if no_launch {
        println!("\nWorkspace ready. To use manually:");
        println!("  cd {}", staging_path.display());
        println!("  {} {}", agent_config.command, shell_quote(&prompt));
        println!();
        println!("When done, build the PR:");
        println!("  ta pr build {}", goal_id);
        println!("  # Or: ta pr build --latest");
        return Ok(());
    }

    // 3. Launch the agent in the staging directory.
    println!(
        "\nLaunching {} in staging workspace...",
        agent_config.command
    );
    println!("  Working dir: {}", staging_path.display());
    println!();

    let status = launch_agent(&agent_config, &staging_path, &prompt);

    match status {
        Ok(exit) => {
            if exit.success() {
                println!("\nAgent exited. Building PR package...");
            } else {
                println!(
                    "\nAgent exited with status {}. Building PR package anyway...",
                    exit
                );
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                println!(
                    "\n'{}' command not found. To use manually:",
                    agent_config.command
                );
                println!("  cd {}", staging_path.display());
                println!("  {} {}", agent_config.command, shell_quote(&prompt));
                println!();
                println!("When done, build the PR:");
                println!("  ta pr build {}", goal_id);
                return Ok(());
            }
            return Err(anyhow::anyhow!(
                "Failed to launch {}: {}",
                agent_config.command,
                e
            ));
        }
    }

    // 4. Restore original CLAUDE.md before diffing (removes TA injection).
    if agent_config.injects_context_file {
        restore_claude_md(&staging_path)?;
    }

    // 5. Build PR package from the diff.
    super::pr::execute(
        &super::pr::PrCommands::Build {
            goal_id: goal_id.clone(),
            summary: format!("Changes from goal: {}", title),
            latest: false,
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

// ── Agent launch ────────────────────────────────────────────────

/// Launch an agent process with template-substituted arguments.
fn launch_agent(
    config: &AgentLaunchConfig,
    staging_path: &Path,
    prompt: &str,
) -> std::io::Result<std::process::ExitStatus> {
    let mut cmd = std::process::Command::new(&config.command);
    cmd.current_dir(staging_path);

    for arg_template in config.args_template {
        let arg = arg_template.replace("{prompt}", prompt);
        cmd.arg(arg);
    }

    cmd.status()
}

/// Simple shell quoting for display purposes.
fn shell_quote(s: &str) -> String {
    if s.contains(' ') || s.contains('\n') {
        format!("\"{}\"", s.replace('\"', "\\\""))
    } else {
        s.to_string()
    }
}

// ── CLAUDE.md injection and restoration ─────────────────────────

const CLAUDE_MD_BACKUP: &str = ".ta/claude_md_original";
const NO_ORIGINAL_SENTINEL: &str = "__TA_NO_ORIGINAL__";

/// Inject a CLAUDE.md file into the staging workspace to orient the agent.
/// Saves the original content to `.ta/claude_md_original` for later restoration.
fn inject_claude_md(staging_path: &Path, title: &str, goal_id: &str) -> anyhow::Result<()> {
    let claude_md_path = staging_path.join("CLAUDE.md");

    // Read and save original content.
    let original_content = if claude_md_path.exists() {
        std::fs::read_to_string(&claude_md_path)?
    } else {
        NO_ORIGINAL_SENTINEL.to_string()
    };

    // Save backup to .ta/ in staging (excluded from copy and diff).
    let backup_dir = staging_path.join(".ta");
    std::fs::create_dir_all(&backup_dir)?;
    std::fs::write(staging_path.join(CLAUDE_MD_BACKUP), &original_content)?;

    // Build injected content.
    let existing_section = if original_content == NO_ORIGINAL_SENTINEL {
        String::new()
    } else {
        original_content
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
        title, goal_id, existing_section
    );

    std::fs::write(&claude_md_path, injected)?;
    Ok(())
}

/// Restore the original CLAUDE.md content before computing diffs.
/// This removes TA's injection so it doesn't appear in PR packages.
fn restore_claude_md(staging_path: &Path) -> anyhow::Result<()> {
    let backup_path = staging_path.join(CLAUDE_MD_BACKUP);
    let claude_md_path = staging_path.join("CLAUDE.md");

    if !backup_path.exists() {
        return Ok(()); // No backup — nothing to restore.
    }

    let original = std::fs::read_to_string(&backup_path)?;

    if original == NO_ORIGINAL_SENTINEL {
        // CLAUDE.md didn't exist originally — remove it.
        if claude_md_path.exists() {
            std::fs::remove_file(&claude_md_path)?;
        }
    } else {
        // Restore original content.
        std::fs::write(&claude_md_path, original)?;
    }

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

        // Run with --no-launch to avoid actually starting the agent.
        execute(
            &config,
            "Test goal",
            "claude-code",
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

        // Verify backup was saved.
        let backup =
            std::fs::read_to_string(goals[0].workspace_path.join(CLAUDE_MD_BACKUP)).unwrap();
        assert_eq!(backup, "# Existing project instructions\n");
    }

    #[test]
    fn inject_and_restore_claude_md_roundtrip() {
        let staging = TempDir::new().unwrap();
        let original = "# My Project\nExisting instructions.\n";
        std::fs::write(staging.path().join("CLAUDE.md"), original).unwrap();

        inject_claude_md(staging.path(), "Fix bug", "goal-123").unwrap();

        // Verify injection happened.
        let injected = std::fs::read_to_string(staging.path().join("CLAUDE.md")).unwrap();
        assert!(injected.contains("Trusted Autonomy"));
        assert!(injected.contains("Fix bug"));
        assert!(injected.contains("Existing instructions"));

        // Restore.
        restore_claude_md(staging.path()).unwrap();

        // Verify original content is back.
        let restored = std::fs::read_to_string(staging.path().join("CLAUDE.md")).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn restore_removes_claude_md_if_not_originally_present() {
        let staging = TempDir::new().unwrap();
        // No CLAUDE.md exists initially.

        inject_claude_md(staging.path(), "New goal", "goal-456").unwrap();

        // CLAUDE.md was created by injection.
        assert!(staging.path().join("CLAUDE.md").exists());

        // Restore should remove it.
        restore_claude_md(staging.path()).unwrap();
        assert!(!staging.path().join("CLAUDE.md").exists());
    }

    #[test]
    fn agent_config_returns_correct_launch_config() {
        let claude = agent_launch_config("claude-code");
        assert_eq!(claude.command, "claude");
        assert!(claude.injects_context_file);
        assert!(claude
            .args_template
            .contains(&"--dangerously-skip-permissions"));

        let codex = agent_launch_config("codex");
        assert_eq!(codex.command, "codex");
        assert!(!codex.injects_context_file);

        let unknown = agent_launch_config("my-custom-agent");
        assert_eq!(unknown.command, "my-custom-agent");
        assert!(unknown.args_template.is_empty());
    }
}
