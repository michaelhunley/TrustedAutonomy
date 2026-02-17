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

/// Agent launch descriptor.
/// Loaded from YAML config files (with hard-coded fallbacks for built-in agents).
///
/// Config search order:
///   1. `.ta/agents/<agent-id>.yaml`       (project override)
///   2. `~/.config/ta/agents/<agent-id>.yaml`  (user override)
///   3. `<binary-dir>/agents/<agent-id>.yaml`  (shipped defaults)
///   4. Hard-coded fallback
#[derive(serde::Deserialize, Clone, Debug)]
struct AgentLaunchConfig {
    /// The command to execute (e.g., "claude", "codex").
    command: String,
    /// Arguments to pass. `{prompt}` is replaced with the goal text.
    args_template: Vec<String>,
    /// Whether this agent reads CLAUDE.md for context injection.
    #[serde(default)]
    injects_context_file: bool,
    /// Whether to inject .claude/settings.local.json with TA permissions.
    #[serde(default)]
    injects_settings: bool,
    /// Optional command to run before the main agent launch (e.g., init).
    #[serde(default)]
    pre_launch: Option<PreLaunchConfig>,
    /// Environment variables to set for the agent process.
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
    /// Human-readable name (informational only, used by `ta agent list` in future).
    #[serde(default)]
    #[allow(dead_code)]
    name: Option<String>,
    /// Description of the agent (informational only, used by `ta agent list` in future).
    #[serde(default)]
    #[allow(dead_code)]
    description: Option<String>,
}

/// Pre-launch command configuration (deserialized from YAML).
#[derive(serde::Deserialize, Clone, Debug)]
struct PreLaunchConfig {
    command: String,
    args: Vec<String>,
}

/// Try to load agent config from YAML files, falling back to hard-coded defaults.
fn agent_launch_config(agent_id: &str, source_dir: Option<&Path>) -> AgentLaunchConfig {
    // Try YAML configs in priority order.
    if let Some(config) = load_agent_yaml(agent_id, source_dir) {
        return config;
    }
    // Fall back to built-in defaults.
    builtin_agent_config(agent_id)
}

/// Search for an agent YAML config in standard locations.
fn load_agent_yaml(agent_id: &str, source_dir: Option<&Path>) -> Option<AgentLaunchConfig> {
    let filename = format!("{}.yaml", agent_id);

    // 1. Project override: .ta/agents/<agent-id>.yaml
    if let Some(source) = source_dir {
        let project_path = source.join(".ta").join("agents").join(&filename);
        if let Some(config) = try_load_yaml(&project_path) {
            return Some(config);
        }
    }

    // 2. User override: ~/.config/ta/agents/<agent-id>.yaml
    if let Some(home) = dirs_path() {
        let user_path = home
            .join(".config")
            .join("ta")
            .join("agents")
            .join(&filename);
        if let Some(config) = try_load_yaml(&user_path) {
            return Some(config);
        }
    }

    // 3. Shipped defaults: <binary-dir>/agents/<agent-id>.yaml
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            let shipped_path = bin_dir.join("agents").join(&filename);
            if let Some(config) = try_load_yaml(&shipped_path) {
                return Some(config);
            }
        }
    }

    None
}

/// Attempt to read and parse a single YAML config file.
fn try_load_yaml(path: &Path) -> Option<AgentLaunchConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_yaml::from_str(&content).ok()
}

/// Get home directory (cross-platform).
fn dirs_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

/// Hard-coded built-in agent configs (fallback when no YAML file is found).
fn builtin_agent_config(agent_id: &str) -> AgentLaunchConfig {
    match agent_id {
        "claude-code" => AgentLaunchConfig {
            command: "claude".to_string(),
            args_template: vec!["{prompt}".to_string()],
            injects_context_file: true,
            injects_settings: true,
            pre_launch: None,
            env: Default::default(),
            name: Some("claude-code".to_string()),
            description: Some("Anthropic's Claude Code CLI".to_string()),
        },
        "codex" => AgentLaunchConfig {
            command: "codex".to_string(),
            args_template: vec![
                "--approval-mode".to_string(),
                "full-auto".to_string(),
                "{prompt}".to_string(),
            ],
            injects_context_file: false,
            injects_settings: false,
            pre_launch: None,
            env: Default::default(),
            name: Some("codex".to_string()),
            description: Some("OpenAI's Codex CLI".to_string()),
        },
        "claude-flow" => AgentLaunchConfig {
            command: "npx".to_string(),
            args_template: vec![
                "claude-flow@alpha".to_string(),
                "hive-mind".to_string(),
                "spawn".to_string(),
                "{prompt}".to_string(),
                "--claude".to_string(),
            ],
            injects_context_file: true,
            injects_settings: true,
            pre_launch: Some(PreLaunchConfig {
                command: "npx".to_string(),
                args: vec![
                    "claude-flow@alpha".to_string(),
                    "hive-mind".to_string(),
                    "init".to_string(),
                ],
            }),
            env: Default::default(),
            name: Some("claude-flow".to_string()),
            description: Some("Claude Flow multi-agent orchestration".to_string()),
        },
        _ => AgentLaunchConfig {
            command: agent_id.to_string(),
            args_template: vec![],
            injects_context_file: false,
            injects_settings: false,
            pre_launch: None,
            env: Default::default(),
            name: None,
            description: None,
        },
    }
}

// ── Public API ──────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn execute(
    config: &GatewayConfig,
    title: &str,
    agent: &str,
    source: Option<&Path>,
    objective: &str,
    phase: Option<&str>,
    follow_up: Option<&Option<String>>,
    objective_file: Option<&Path>,
    no_launch: bool,
) -> anyhow::Result<()> {
    let agent_config = agent_launch_config(agent, source);

    // 1. Start the goal (creates overlay workspace).
    let goal_store = GoalRunStore::new(&config.goals_dir)?;

    super::goal::execute(
        &super::goal::GoalCommands::Start {
            title: title.to_string(),
            source: source.map(|p| p.to_path_buf()),
            objective: objective.to_string(),
            agent: agent.to_string(),
            phase: phase.map(|p| p.to_string()),
            follow_up: follow_up.cloned(),
            objective_file: objective_file.map(|p| p.to_path_buf()),
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
            goal.parent_goal_id,
            &goal_store,
            config,
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
        if let Some(ref pre) = agent_config.pre_launch {
            println!(
                "  {} {}  # required init step",
                pre.command,
                pre.args.join(" ")
            );
        }
        println!("  {} {}", agent_config.command, shell_quote(&prompt));
        println!();
        println!("When done, build the draft:");
        println!("  ta draft build {}", goal_id);
        println!("  # Or: ta draft build --latest");
        return Ok(());
    }

    // 3. Run pre-launch command if needed (e.g., hive-mind init).
    if let Some(ref pre) = agent_config.pre_launch {
        println!(
            "\nRunning pre-launch: {} {}...",
            pre.command,
            pre.args.join(" ")
        );
        let pre_status = std::process::Command::new(&pre.command)
            .args(&pre.args)
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
                    pre.command,
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
                println!("\nAgent exited. Building draft...");
            } else {
                println!(
                    "\nAgent exited with status {}. Building draft anyway...",
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
                println!("When done, build the draft:");
                println!("  ta draft build {}", goal_id);
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
    println!("  ta draft list");
    println!("  ta draft view <draft-id>");
    println!("  ta draft approve <draft-id>");
    println!("  ta draft apply <draft-id> --git-commit");

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

    for arg_template in &config.args_template {
        let arg = arg_template.replace("{prompt}", prompt);
        cmd.arg(arg);
    }

    // Set agent-specific environment variables from config.
    for (key, value) in &config.env {
        cmd.env(key, value);
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

/// Build a parent goal context section for CLAUDE.md injection.
/// Returns empty string if no parent goal or if parent's PR is not available.
fn build_parent_context_section(
    parent_goal_id: Option<uuid::Uuid>,
    goal_store: &ta_goal::GoalRunStore,
    config: &GatewayConfig,
) -> String {
    let parent_id = match parent_goal_id {
        Some(id) => id,
        None => return String::new(),
    };

    let parent_goal = match goal_store.get(parent_id) {
        Ok(Some(g)) => g,
        _ => return String::new(),
    };

    let mut context = format!(
        "\n## Follow-Up Context\n\nThis is a follow-up goal building on:\n\
         **Parent Goal:** {} ({})\n\
         **Parent Objective:** {}\n",
        parent_goal.title, parent_id, parent_goal.objective
    );

    // If parent has a PR, include artifact dispositions and discuss items.
    if let Some(pr_id) = parent_goal.pr_package_id {
        use crate::commands::pr::load_package;
        if let Ok(parent_pr) = load_package(config, pr_id) {
            let approved = parent_pr
                .changes
                .artifacts
                .iter()
                .filter(|a| {
                    matches!(
                        a.disposition,
                        ta_changeset::pr_package::ArtifactDisposition::Approved
                    )
                })
                .count();
            let rejected = parent_pr
                .changes
                .artifacts
                .iter()
                .filter(|a| {
                    matches!(
                        a.disposition,
                        ta_changeset::pr_package::ArtifactDisposition::Rejected
                    )
                })
                .count();
            let discuss = parent_pr
                .changes
                .artifacts
                .iter()
                .filter(|a| {
                    matches!(
                        a.disposition,
                        ta_changeset::pr_package::ArtifactDisposition::Discuss
                    )
                })
                .count();

            context.push_str(&format!(
                "\n**Parent PR Status:** {} ({} approved, {} rejected, {} discuss)\n",
                parent_pr.status, approved, rejected, discuss
            ));

            // List discuss items with their rationale.
            let discuss_items: Vec<_> = parent_pr
                .changes
                .artifacts
                .iter()
                .filter(|a| {
                    matches!(
                        a.disposition,
                        ta_changeset::pr_package::ArtifactDisposition::Discuss
                    )
                })
                .collect();

            if !discuss_items.is_empty() {
                context.push_str("\n### Items for Discussion:\n");
                for artifact in discuss_items {
                    context.push_str(&format!("- {}", artifact.resource_uri));
                    if let Some(ref why) = artifact.rationale {
                        context.push_str(&format!(" — {}", why));
                    }
                    context.push('\n');
                }
            }
        }
    }

    context
}

/// Inject a CLAUDE.md file into the staging workspace to orient the agent.
/// Saves the original content to `.ta/claude_md_original` for later restoration.
#[allow(clippy::too_many_arguments)]
fn inject_claude_md(
    staging_path: &Path,
    title: &str,
    goal_id: &str,
    plan_phase: Option<&str>,
    source_dir: Option<&Path>,
    parent_goal_id: Option<uuid::Uuid>,
    goal_store: &ta_goal::GoalRunStore,
    config: &GatewayConfig,
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

    // Build parent context section if this is a follow-up goal.
    let parent_section = build_parent_context_section(parent_goal_id, goal_store, config);

    let injected = format!(
        r#"# Trusted Autonomy — Mediated Goal

You are working on a TA-mediated goal in a staging workspace.

**Goal:** {}
**Goal ID:** {}
{}{}
## How this works

- This directory is a copy of the original project
- Work normally — Read, Write, Edit, Bash all work as expected
- When you're done, just exit. TA will diff your changes and create a PR package
- The human reviewer will see exactly what you changed and why

## Important

- Do NOT modify files outside this directory
- All your changes will be captured as a PR for review

## Before You Exit — Change Summary (REQUIRED)

You MUST create `.ta/change_summary.json` before exiting. The human reviewer relies on this to understand your work. Every changed file needs a clear "what I did" and "why" — reviewers who don't understand a change will reject it.

```json
{{
  "summary": "Brief description of all changes made in this session",
  "changes": [
    {{
      "path": "relative/path/to/file",
      "action": "modified|created|deleted",
      "what": "Specific description of what was changed in this target",
      "why": "Why this change was needed (motivation, not just restating what)",
      "independent": true,
      "depends_on": [],
      "depended_by": []
    }}
  ],
  "dependency_notes": "Human-readable explanation of which changes are coupled and why"
}}
```

Rules for per-target descriptions:
- **`what`** (REQUIRED): Describe specifically what you changed. NOT "updated file" — instead "Added JWT validation middleware with RS256 signature verification" or "Removed deprecated session-cookie auth fallback". The reviewer sees this as the primary description for each changed file.
- **`why`**: The motivation, not a restatement of what. "Security audit flagged session cookies as vulnerable" not "To add JWT validation".
- For lockfiles, config files, and generated files: still provide `what` (e.g., "Added jsonwebtoken v9.3 dependency") — don't leave them blank.
- `independent`: true if this change can be applied or reverted without affecting other changes
- `depends_on`: list of other file paths this change requires (e.g., if you add a function call, it depends on the file where the function is defined)
- `depended_by`: list of other file paths that would break if this change is reverted
- Be honest about dependencies — the reviewer uses this to decide which changes to accept individually

## Documentation Updates

If your changes affect user-facing behavior (new commands, changed flags, new config options, workflow changes):
- Update `docs/USAGE.md` with the new/changed functionality
- Keep the tone consumer-friendly (no internal implementation details)
- Update version references if they exist in the docs
- Update the `CLAUDE.md` "Current State" section if the test count changes

---

{}
"#,
        title, goal_id, plan_section, parent_section, existing_section
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
            None,
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
        let config = GatewayConfig::for_project(staging.path());
        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();

        std::fs::write(
            staging.path().join("CLAUDE.md"),
            "# Existing project instructions\n",
        )
        .unwrap();

        inject_claude_md(
            staging.path(),
            "Test goal",
            "goal-123",
            None,
            None,
            None,
            &goal_store,
            &config,
        )
        .unwrap();

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
        let config = GatewayConfig::for_project(staging.path());
        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        let original = "# My Project\nExisting instructions.\n";
        std::fs::write(staging.path().join("CLAUDE.md"), original).unwrap();

        inject_claude_md(
            staging.path(),
            "Fix bug",
            "goal-123",
            None,
            None,
            None,
            &goal_store,
            &config,
        )
        .unwrap();

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
        let config = GatewayConfig::for_project(staging.path());
        let goal_store = GoalRunStore::new(&config.goals_dir).unwrap();
        // No CLAUDE.md exists initially.

        inject_claude_md(
            staging.path(),
            "New goal",
            "goal-456",
            None,
            None,
            None,
            &goal_store,
            &config,
        )
        .unwrap();

        // CLAUDE.md was created by injection.
        assert!(staging.path().join("CLAUDE.md").exists());

        // Restore should remove it.
        restore_claude_md(staging.path()).unwrap();
        assert!(!staging.path().join("CLAUDE.md").exists());
    }

    #[test]
    fn agent_config_returns_correct_launch_config() {
        // Pass None for source_dir — tests use built-in fallbacks (no YAML files).
        let claude = agent_launch_config("claude-code", None);
        assert_eq!(claude.command, "claude");
        assert!(claude.injects_context_file);
        assert!(claude.injects_settings);
        assert!(!claude
            .args_template
            .contains(&"--dangerously-skip-permissions".to_string()));

        let codex = agent_launch_config("codex", None);
        assert_eq!(codex.command, "codex");
        assert!(!codex.injects_context_file);
        assert!(!codex.injects_settings);

        let flow = agent_launch_config("claude-flow", None);
        assert_eq!(flow.command, "npx");
        assert!(flow.injects_context_file);
        assert!(flow.injects_settings);
        assert!(flow
            .args_template
            .contains(&"claude-flow@alpha".to_string()));
        assert!(flow.args_template.contains(&"hive-mind".to_string()));
        assert!(flow.args_template.contains(&"--claude".to_string()));
        let pre = flow.pre_launch.expect("claude-flow should have pre_launch");
        assert_eq!(pre.command, "npx");
        assert!(pre.args.contains(&"hive-mind".to_string()));
        assert!(pre.args.contains(&"init".to_string()));

        let unknown = agent_launch_config("my-custom-agent", None);
        assert_eq!(unknown.command, "my-custom-agent");
        assert!(unknown.args_template.is_empty());
        assert!(unknown.pre_launch.is_none());
        assert!(!unknown.injects_settings);
    }

    #[test]
    fn agent_config_loads_from_yaml() {
        let project = TempDir::new().unwrap();
        let agents_dir = project.path().join(".ta").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        let yaml = r#"
name: test-agent
description: "A test agent"
command: my-test-cmd
args_template:
  - "--flag"
  - "{prompt}"
injects_context_file: true
injects_settings: false
env:
  MY_VAR: "hello"
"#;
        std::fs::write(agents_dir.join("test-agent.yaml"), yaml).unwrap();

        let config = agent_launch_config("test-agent", Some(project.path()));
        assert_eq!(config.command, "my-test-cmd");
        assert_eq!(config.args_template, vec!["--flag", "{prompt}"]);
        assert!(config.injects_context_file);
        assert!(!config.injects_settings);
        assert!(config.pre_launch.is_none());
        assert_eq!(config.env.get("MY_VAR").unwrap(), "hello");
    }

    #[test]
    fn agent_config_yaml_with_pre_launch() {
        let project = TempDir::new().unwrap();
        let agents_dir = project.path().join(".ta").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        let yaml = r#"
name: flow-test
command: npx
args_template: ["{prompt}"]
pre_launch:
  command: npx
  args: ["flow", "init"]
"#;
        std::fs::write(agents_dir.join("flow-test.yaml"), yaml).unwrap();

        let config = agent_launch_config("flow-test", Some(project.path()));
        assert_eq!(config.command, "npx");
        let pre = config.pre_launch.expect("should have pre_launch");
        assert_eq!(pre.command, "npx");
        assert_eq!(pre.args, vec!["flow", "init"]);
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
