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

use super::plan;

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
    /// Whether to inject .claude/settings.local.json with TA permissions.
    injects_settings: bool,
    /// Optional command to run before the main agent launch (e.g., init).
    /// (command, args) — runs in the staging directory.
    pre_launch: Option<(&'static str, &'static [&'static str])>,
}

/// Look up the built-in launch config for an agent.
fn agent_launch_config(agent_id: &str) -> AgentLaunchConfig {
    match agent_id {
        "claude-code" => AgentLaunchConfig {
            command: "claude".to_string(),
            args_template: &["{prompt}"],
            injects_context_file: true,
            injects_settings: true,
            pre_launch: None,
        },
        "codex" => AgentLaunchConfig {
            command: "codex".to_string(),
            args_template: &["--approval-mode", "full-auto", "{prompt}"],
            injects_context_file: false,
            injects_settings: false,
            pre_launch: None,
        },
        "claude-flow" => AgentLaunchConfig {
            command: "npx".to_string(),
            args_template: &[
                "claude-flow@alpha",
                "hive-mind",
                "spawn",
                "{prompt}",
                "--claude",
            ],
            injects_context_file: true,
            injects_settings: true,
            pre_launch: Some(("npx", &["claude-flow@alpha", "hive-mind", "init"])),
        },
        _ => AgentLaunchConfig {
            command: agent_id.to_string(),
            args_template: &[],
            injects_context_file: false,
            injects_settings: false,
            pre_launch: None,
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
    phase: Option<&str>,
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
            phase: phase.map(|p| p.to_string()),
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

    // 2. Inject context and settings into the staging workspace.
    if agent_config.injects_context_file {
        inject_claude_md(
            &staging_path,
            title,
            &goal_id,
            goal.plan_phase.as_deref(),
            goal.source_dir.as_deref(),
        )?;
    }
    if agent_config.injects_settings {
        inject_claude_settings(&staging_path, source)?;
    }

    // Build the prompt string.
    let prompt = if objective.is_empty() {
        format!("Implement: {}", title)
    } else {
        format!("{}\n\nObjective: {}", title, objective)
    };

    if no_launch {
        // Restore injected files — user will run the agent manually,
        // so injected context stays only if they re-run `ta run`.
        if agent_config.injects_context_file {
            restore_claude_md(&staging_path)?;
        }
        if agent_config.injects_settings {
            restore_claude_settings(&staging_path)?;
        }

        println!("\nWorkspace ready. To use manually:");
        println!("  cd {}", staging_path.display());
        if let Some((cmd, args)) = agent_config.pre_launch {
            println!("  {} {}  # required init step", cmd, args.join(" "));
        }
        println!("  {} {}", agent_config.command, shell_quote(&prompt));
        println!();
        println!("When done, build the PR:");
        println!("  ta pr build {}", goal_id);
        println!("  # Or: ta pr build --latest");
        return Ok(());
    }

    // 3. Run pre-launch command if needed (e.g., hive-mind init).
    if let Some((cmd, args)) = agent_config.pre_launch {
        println!("\nRunning pre-launch: {} {}...", cmd, args.join(" "));
        let pre_status = std::process::Command::new(cmd)
            .args(args)
            .current_dir(&staging_path)
            .status();
        match pre_status {
            Ok(exit) if exit.success() => {}
            Ok(exit) => {
                return Err(anyhow::anyhow!(
                    "Pre-launch command exited with status {}",
                    exit
                ));
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to run pre-launch command '{}': {}",
                    cmd,
                    e
                ));
            }
        }
    }

    // 4. Launch the agent in the staging directory.
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
                // Restore injected files before returning — agent won't run.
                if agent_config.injects_context_file {
                    restore_claude_md(&staging_path)?;
                }
                if agent_config.injects_settings {
                    restore_claude_settings(&staging_path)?;
                }

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

    // 5. Restore injected files before diffing (removes TA injection).
    if agent_config.injects_context_file {
        restore_claude_md(&staging_path)?;
    }
    if agent_config.injects_settings {
        restore_claude_settings(&staging_path)?;
    }

    // 6. Build PR package from the diff.
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

// ── Claude Code settings injection ──────────────────────────────
//
// Instead of --dangerously-skip-permissions, TA injects a
// .claude/settings.local.json that allows all standard tools but
// denies patterns from the forbidden-tools list. This makes the
// agent work uninterrupted in the staging sandbox while keeping
// community-maintained safety rails.

const SETTINGS_BACKUP: &str = ".ta/claude_settings_original";
const SETTINGS_REL_PATH: &str = ".claude/settings.local.json";
const FORBIDDEN_TOOLS_FILE: &str = ".ta-forbidden-tools";

/// Tools to allow in the injected Claude Code settings.
const DEFAULT_ALLOWED_TOOLS: &[&str] = &[
    "Bash(*)",
    "Read(*)",
    "Write(*)",
    "Edit(*)",
    "MultiEdit(*)",
    "Glob(*)",
    "Grep(*)",
    "WebFetch(*)",
    "WebSearch(*)",
    "NotebookEdit(*)",
    "Task(*)",
    "Skill(*)",
    "TodoRead(*)",
    "TodoWrite(*)",
];

/// Built-in forbidden tool patterns — community-maintained deny list.
/// These are always denied even in TA staging workspaces.
/// Add patterns here as the community identifies dangerous tools/commands.
const DEFAULT_FORBIDDEN_TOOLS: &[&str] = &[];

/// Load forbidden tool patterns from the project's `.ta-forbidden-tools` file.
/// Returns an empty vec if the file doesn't exist.
fn load_forbidden_tools(source_dir: Option<&Path>) -> Vec<String> {
    let mut patterns: Vec<String> = DEFAULT_FORBIDDEN_TOOLS
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    if let Some(source) = source_dir {
        let path = source.join(FORBIDDEN_TOOLS_FILE);
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                for line in contents.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        patterns.push(trimmed.to_string());
                    }
                }
            }
        }
    }

    patterns
}

/// Inject .claude/settings.local.json with TA permissions.
/// Allows all standard tools, denies forbidden patterns.
fn inject_claude_settings(staging_path: &Path, source_dir: Option<&Path>) -> anyhow::Result<()> {
    let settings_path = staging_path.join(SETTINGS_REL_PATH);

    // Read and save original content.
    let original_content = if settings_path.exists() {
        std::fs::read_to_string(&settings_path)?
    } else {
        NO_ORIGINAL_SENTINEL.to_string()
    };

    // Save backup.
    let backup_dir = staging_path.join(".ta");
    std::fs::create_dir_all(&backup_dir)?;
    std::fs::write(staging_path.join(SETTINGS_BACKUP), &original_content)?;

    // Build allow and deny lists.
    let allow: Vec<String> = DEFAULT_ALLOWED_TOOLS
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect();
    let forbidden = load_forbidden_tools(source_dir);
    let deny: Vec<String> = forbidden.iter().map(|s| format!("\"{}\"", s)).collect();

    let settings_json = format!(
        r#"{{
  "_comment": "Injected by Trusted Autonomy. Agent works in a staging sandbox — all changes require human review before applying. See .ta-forbidden-tools to deny specific patterns.",
  "permissions": {{
    "allow": [
      {}
    ],
    "deny": [
      {}
    ]
  }}
}}"#,
        allow.join(",\n      "),
        deny.join(",\n      ")
    );

    // Ensure .claude/ directory exists.
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&settings_path, settings_json)?;
    Ok(())
}

/// Restore original .claude/settings.local.json before diffing.
fn restore_claude_settings(staging_path: &Path) -> anyhow::Result<()> {
    let backup_path = staging_path.join(SETTINGS_BACKUP);
    let settings_path = staging_path.join(SETTINGS_REL_PATH);

    if !backup_path.exists() {
        return Ok(());
    }

    let original = std::fs::read_to_string(&backup_path)?;

    if original == NO_ORIGINAL_SENTINEL {
        // Settings file didn't exist originally — remove it and the .claude/ dir if empty.
        if settings_path.exists() {
            std::fs::remove_file(&settings_path)?;
        }
        if let Some(parent) = settings_path.parent() {
            // Only remove .claude/ if it's empty (don't delete user's other configs).
            let _ = std::fs::remove_dir(parent);
        }
    } else {
        std::fs::write(&settings_path, original)?;
    }

    Ok(())
}

// ── CLAUDE.md injection and restoration ─────────────────────────

const CLAUDE_MD_BACKUP: &str = ".ta/claude_md_original";
const NO_ORIGINAL_SENTINEL: &str = "__TA_NO_ORIGINAL__";

/// Build a plan context section for CLAUDE.md injection.
/// Returns empty string if no PLAN.md or no phase specified.
fn build_plan_section(plan_phase: Option<&str>, source_dir: Option<&Path>) -> String {
    let source = match source_dir {
        Some(s) => s,
        None => return String::new(),
    };

    let phases = match plan::load_plan(source) {
        Ok(p) => p,
        Err(_) => return String::new(),
    };

    if phases.is_empty() {
        return String::new();
    }

    let checklist = plan::format_plan_checklist(&phases, plan_phase);

    let current_line = if let Some(phase_id) = plan_phase {
        if let Some(phase) = phases.iter().find(|p| p.id == phase_id) {
            format!(
                "\n**You are working on Phase {} — {}.**\n",
                phase.id, phase.title
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    format!(
        "\n## Plan Context\n{}\nPlan progress:\n{}\n",
        current_line, checklist
    )
}

/// Inject a CLAUDE.md file into the staging workspace to orient the agent.
/// Saves the original content to `.ta/claude_md_original` for later restoration.
fn inject_claude_md(
    staging_path: &Path,
    title: &str,
    goal_id: &str,
    plan_phase: Option<&str>,
    source_dir: Option<&Path>,
) -> anyhow::Result<()> {
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

    // Build plan context section if PLAN.md exists in source.
    let plan_section = build_plan_section(plan_phase, source_dir);

    let injected = format!(
        r#"# Trusted Autonomy — Mediated Goal

You are working on a TA-mediated goal in a staging workspace.

**Goal:** {}
**Goal ID:** {}
{}
## How this works

- This directory is a copy of the original project
- Work normally — Read, Write, Edit, Bash all work as expected
- When you're done, just exit. TA will diff your changes and create a PR package
- The human reviewer will see exactly what you changed and why

## Important

- Do NOT modify files outside this directory
- All your changes will be captured as a PR for review

## Before You Exit — Change Summary

Before exiting, create a file `.ta/change_summary.json` with this structure:
```json
{{
  "summary": "Brief description of all changes",
  "changes": [
    {{
      "path": "relative/path/to/file",
      "action": "modified|created|deleted",
      "why": "Why this change was needed",
      "independent": true,
      "depends_on": [],
      "depended_by": []
    }}
  ],
  "dependency_notes": "Human-readable explanation of which changes are coupled and why"
}}
```

Rules for the summary:
- `independent`: true if this change can be applied or reverted without affecting other changes
- `depends_on`: list of other file paths this change requires (e.g., if you add a function call, it depends on the file where the function is defined)
- `depended_by`: list of other file paths that would break if this change is reverted
- Be honest about dependencies — the reviewer uses this to decide which changes to accept individually

---

{}
"#,
        title, goal_id, plan_section, existing_section
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
    fn run_creates_goal_and_restores_on_no_launch() {
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
            None,
            true,
        )
        .unwrap();

        // Verify goal was created.
        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let goals = goal_store.list().unwrap();
        assert_eq!(goals.len(), 1);

        // With --no-launch, injected files should be restored so they
        // don't contaminate a subsequent `ta pr build` diff.
        let claude_md = std::fs::read_to_string(goals[0].workspace_path.join("CLAUDE.md")).unwrap();
        assert_eq!(claude_md, "# Existing project instructions\n");

        // Settings should also be restored (removed since it didn't exist).
        assert!(!goals[0].workspace_path.join(SETTINGS_REL_PATH).exists());
    }

    #[test]
    fn run_injects_context_for_agent() {
        // Verify that inject + restore roundtrip works for the agent path.
        let staging = TempDir::new().unwrap();
        std::fs::write(
            staging.path().join("CLAUDE.md"),
            "# Existing project instructions\n",
        )
        .unwrap();

        inject_claude_md(staging.path(), "Test goal", "goal-123", None, None).unwrap();

        // Verify CLAUDE.md was injected.
        let claude_md = std::fs::read_to_string(staging.path().join("CLAUDE.md")).unwrap();
        assert!(claude_md.contains("Trusted Autonomy"));
        assert!(claude_md.contains("Test goal"));
        assert!(claude_md.contains("Existing project instructions"));

        // Verify backup was saved.
        let backup = std::fs::read_to_string(staging.path().join(CLAUDE_MD_BACKUP)).unwrap();
        assert_eq!(backup, "# Existing project instructions\n");

        inject_claude_settings(staging.path(), None).unwrap();

        // Verify settings.local.json was injected.
        let settings = std::fs::read_to_string(staging.path().join(SETTINGS_REL_PATH)).unwrap();
        assert!(settings.contains("Trusted Autonomy"));
        assert!(settings.contains("Bash(*)"));
    }

    #[test]
    fn inject_and_restore_claude_md_roundtrip() {
        let staging = TempDir::new().unwrap();
        let original = "# My Project\nExisting instructions.\n";
        std::fs::write(staging.path().join("CLAUDE.md"), original).unwrap();

        inject_claude_md(staging.path(), "Fix bug", "goal-123", None, None).unwrap();

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

        inject_claude_md(staging.path(), "New goal", "goal-456", None, None).unwrap();

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
        assert!(claude.injects_settings);
        // No --dangerously-skip-permissions — TA injects settings instead.
        assert!(!claude
            .args_template
            .contains(&"--dangerously-skip-permissions"));

        let codex = agent_launch_config("codex");
        assert_eq!(codex.command, "codex");
        assert!(!codex.injects_context_file);
        assert!(!codex.injects_settings);

        let flow = agent_launch_config("claude-flow");
        assert_eq!(flow.command, "npx");
        assert!(flow.injects_context_file);
        assert!(flow.injects_settings);
        assert!(flow.args_template.contains(&"claude-flow@alpha"));
        assert!(flow.args_template.contains(&"hive-mind"));
        assert!(flow.args_template.contains(&"--claude"));
        let (pre_cmd, pre_args) = flow.pre_launch.expect("claude-flow should have pre_launch");
        assert_eq!(pre_cmd, "npx");
        assert!(pre_args.contains(&"hive-mind"));
        assert!(pre_args.contains(&"init"));

        let unknown = agent_launch_config("my-custom-agent");
        assert_eq!(unknown.command, "my-custom-agent");
        assert!(unknown.args_template.is_empty());
        assert!(unknown.pre_launch.is_none());
        assert!(!unknown.injects_settings);
    }

    #[test]
    fn inject_and_restore_settings_roundtrip() {
        let staging = TempDir::new().unwrap();

        inject_claude_settings(staging.path(), None).unwrap();

        let settings_path = staging.path().join(SETTINGS_REL_PATH);
        assert!(settings_path.exists());
        let content = std::fs::read_to_string(&settings_path).unwrap();
        assert!(content.contains("Trusted Autonomy"));
        assert!(content.contains("Bash(*)"));
        assert!(content.contains("\"deny\": ["));

        restore_claude_settings(staging.path()).unwrap();
        assert!(!settings_path.exists());
    }

    #[test]
    fn inject_settings_preserves_existing() {
        let staging = TempDir::new().unwrap();
        let claude_dir = staging.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let original = r#"{"customSetting": true}"#;
        std::fs::write(claude_dir.join("settings.local.json"), original).unwrap();

        inject_claude_settings(staging.path(), None).unwrap();

        let injected = std::fs::read_to_string(staging.path().join(SETTINGS_REL_PATH)).unwrap();
        assert!(injected.contains("Trusted Autonomy"));

        restore_claude_settings(staging.path()).unwrap();
        let restored = std::fs::read_to_string(staging.path().join(SETTINGS_REL_PATH)).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn forbidden_tools_loaded_from_file() {
        let project = TempDir::new().unwrap();
        std::fs::write(
            project.path().join(FORBIDDEN_TOOLS_FILE),
            "# Comment line\nBash(rm -rf /*)\n\nBash(curl * | sh)\n",
        )
        .unwrap();

        let forbidden = load_forbidden_tools(Some(project.path()));
        assert_eq!(forbidden.len(), 2);
        assert_eq!(forbidden[0], "Bash(rm -rf /*)");
        assert_eq!(forbidden[1], "Bash(curl * | sh)");
    }

    #[test]
    fn forbidden_tools_empty_when_no_file() {
        let project = TempDir::new().unwrap();
        let forbidden = load_forbidden_tools(Some(project.path()));
        assert!(forbidden.is_empty());
    }

    #[test]
    fn inject_settings_includes_forbidden_tools() {
        let staging = TempDir::new().unwrap();
        let source = TempDir::new().unwrap();
        std::fs::write(
            source.path().join(FORBIDDEN_TOOLS_FILE),
            "Bash(rm -rf /*)\n",
        )
        .unwrap();

        inject_claude_settings(staging.path(), Some(source.path())).unwrap();

        let content = std::fs::read_to_string(staging.path().join(SETTINGS_REL_PATH)).unwrap();
        assert!(content.contains("Bash(rm -rf /*)"));
    }
}
