// constitution.rs — Project behavioral constitution commands (v0.12.0).
//
// `ta constitution init` asks the QA agent to draft a `.ta/constitution.md`
// from the project's PLAN.md, CLAUDE.md, and stated objectives. The result
// is a behavioral contract for AI agents working on this project — defining
// rules, invariants, coding standards, and autonomy boundaries.
//
// This is the "simple" constitution init from §16.6 pull-forward. A single
// agent prompt produces the first draft for human review. The full v0.14.1
// constitution framework (guided UI, incremental sections, versioning) is
// deferred.

use std::path::Path;

use clap::Subcommand;
use ta_mcp_gateway::GatewayConfig;

#[derive(Subcommand, Debug)]
pub enum ConstitutionCommands {
    /// Draft a behavioral constitution for this project.
    ///
    /// Reads PLAN.md, CLAUDE.md (if present), and asks the agent to produce
    /// a `.ta/constitution.md` — a behavioral contract defining rules,
    /// invariants, and autonomy policies for AI agents. The output is a
    /// TA draft for human review before applying.
    Init {
        /// Agent to use for drafting (default: claude-code).
        #[arg(long, default_value = "claude-code")]
        agent: String,
        /// Path to additional context document (PRD, spec, guidelines).
        #[arg(long)]
        from: Option<std::path::PathBuf>,
        /// Skip interactive session — let the agent draft from available docs only.
        #[arg(long)]
        non_interactive: bool,
    },
    /// Show the current .ta/constitution.md if it exists.
    Show,
    /// Check changed files against constitution rules (basic static check).
    ///
    /// Currently checks §4 (inject/restore balance) when s4_scan is enabled
    /// in .ta/workflow.toml. Additional checks will be added in v0.14.1.
    Check {
        /// Draft ID to check (defaults to latest build draft).
        #[arg(long)]
        draft_id: Option<String>,
    },
}

pub fn execute(command: &ConstitutionCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match command {
        ConstitutionCommands::Init {
            agent,
            from,
            non_interactive,
        } => run_init(config, agent, from.as_deref(), *non_interactive),
        ConstitutionCommands::Show => show_constitution(config),
        ConstitutionCommands::Check { draft_id } => check_constitution(config, draft_id.as_deref()),
    }
}

fn run_init(
    config: &GatewayConfig,
    agent: &str,
    from: Option<&Path>,
    non_interactive: bool,
) -> anyhow::Result<()> {
    let project_root = &config.workspace_root;
    let ta_dir = project_root.join(".ta");

    // Warn if constitution already exists.
    let constitution_path = ta_dir.join("constitution.md");
    if constitution_path.exists() {
        println!("Note: .ta/constitution.md already exists.");
        println!("      The agent will be asked to update it rather than create from scratch.");
        println!();
    }

    // Gather project context.
    let plan_content = read_file_if_exists(&project_root.join("PLAN.md"));
    let claude_md_content = read_file_if_exists(&project_root.join("CLAUDE.md"));
    let existing_constitution = read_file_if_exists(&constitution_path);

    let extra_content = if let Some(from_path) = from {
        let resolved = if from_path.is_absolute() {
            from_path.to_path_buf()
        } else {
            project_root.join(from_path)
        };
        if !resolved.exists() {
            anyhow::bail!(
                "Context document not found: {}\n\
                 Provide the full path to your guidelines, spec, or PRD.",
                resolved.display()
            );
        }
        let content = std::fs::read_to_string(&resolved)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", resolved.display(), e))?;
        Some((resolved.display().to_string(), content))
    } else {
        None
    };

    // Build the agent prompt.
    let objective = build_constitution_prompt(
        project_root,
        plan_content.as_deref(),
        claude_md_content.as_deref(),
        existing_constitution.as_deref(),
        extra_content
            .as_ref()
            .map(|(name, content)| (name.as_str(), content.as_str())),
        non_interactive,
    );

    let title = "Draft project behavioral constitution";

    println!("Launching constitution drafting session...");
    println!("  Target: .ta/constitution.md");
    if plan_content.is_some() {
        println!("  Context: PLAN.md found");
    }
    if claude_md_content.is_some() {
        println!("  Context: CLAUDE.md found");
    }
    if let Some((ref name, _)) = extra_content {
        println!("  Context: {}", name);
    }
    println!();

    super::run::execute(
        config,
        Some(title),
        agent,
        None, // source = project root (from config)
        &objective,
        None,             // no phase
        None,             // no follow_up
        None,             // follow_up_draft
        None,             // follow_up_goal
        None,             // no objective file
        false,            // no_launch = false
        !non_interactive, // interactive when not --non-interactive
        false,            // macro_goal = false
        None,             // resume
        false,            // headless = false
        false,            // skip_verify = false
        false,            // quiet = false
        None,             // existing_goal_id
        None,             // workflow = default (single-agent)
    )?;

    println!();
    if constitution_path.exists() {
        println!("Constitution drafted: .ta/constitution.md");
        println!("Review it, then commit it to make it part of your project.");
    } else {
        println!("The agent did not create .ta/constitution.md.");
        println!("Check the draft for details — the agent may have asked a follow-up question.");
    }

    Ok(())
}

fn build_constitution_prompt(
    project_root: &Path,
    plan: Option<&str>,
    claude_md: Option<&str>,
    existing: Option<&str>,
    extra: Option<(&str, &str)>,
    non_interactive: bool,
) -> String {
    let project_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("this project");

    let plan_section = plan
        .map(|p| format!("\n## Project Plan (PLAN.md)\n\n{}\n", truncate(p, 40_000)))
        .unwrap_or_default();

    let claude_section = claude_md
        .map(|c| {
            format!(
                "\n## Project Instructions (CLAUDE.md)\n\n{}\n",
                truncate(c, 20_000)
            )
        })
        .unwrap_or_default();

    let existing_section = existing.map(|e| format!(
        "\n## Existing Constitution (.ta/constitution.md)\n\nUpdate this rather than replace:\n\n{}\n",
        truncate(e, 20_000)
    )).unwrap_or_default();

    let extra_section = extra
        .map(|(name, content)| {
            format!(
                "\n## Additional Context ({})\n\n{}\n",
                name,
                truncate(content, 20_000)
            )
        })
        .unwrap_or_default();

    let interaction_style = if non_interactive {
        "Non-interactive mode: draft the constitution from the provided documents without \
         asking questions. Use reasonable defaults for anything not specified."
    } else {
        "Ask 2-3 focused questions using `ta_ask_human` to understand the project's key \
         behavioral requirements before drafting. Keep questions concise."
    };

    format!(
        r#"You are drafting a behavioral constitution for the project **{name}**.

A behavioral constitution is a Markdown document saved at `.ta/constitution.md` that defines:
- **Rules and invariants** the AI agent must never violate (e.g., "never commit directly to main")
- **Coding standards** specific to this project
- **Autonomy boundaries** — what the agent can decide alone vs. what needs human approval
- **Key patterns** the agent should follow (naming conventions, error handling, etc.)
- **Anti-patterns** to avoid based on past incidents or design decisions

## Your Task

{interaction_style}

Then write `.ta/constitution.md` with clear, actionable rules organized into sections:
1. **Core Invariants** — hard rules that must never be broken
2. **Development Standards** — coding, testing, commit, and PR standards
3. **Autonomy Policy** — when to ask vs. when to proceed
4. **Project-Specific Patterns** — naming, structure, tooling conventions
5. **Known Anti-Patterns** — things that have caused problems before

## Format

Use this structure:
```markdown
# {{Project Name}} — Agent Behavioral Constitution

## Core Invariants
<!-- Rules that must NEVER be violated -->
- **Rule**: Description of what must always/never happen and why.

## Development Standards
<!-- How to write code, tests, commits, PRs -->
- **Standard**: Specific requirement.

## Autonomy Policy
<!-- When to ask vs. proceed -->
- **Proceed without asking**: Description of low-risk actions.
- **Always ask first**: Description of high-risk or irreversible actions.

## Project-Specific Patterns
<!-- Conventions, tooling, naming -->
- **Pattern**: How and why.

## Known Anti-Patterns
<!-- Things to avoid, with brief incident note if applicable -->
- **Anti-pattern**: What not to do and why.
```

Write the constitution based on the project context below. Be specific — generic rules
("write good code") are not useful. Rules should be concrete enough that an AI agent
can check itself against them.
{plan_section}{claude_section}{existing_section}{extra_section}
## Instructions

1. Review the project context above.
2. {interaction_style}
3. Write `.ta/constitution.md` with the completed constitution.
4. Confirm what you wrote using `ta_ask_human` and ask if the human wants any changes.

The project name is: **{name}**"#,
        name = project_name,
        interaction_style = interaction_style,
        plan_section = plan_section,
        claude_section = claude_section,
        existing_section = existing_section,
        extra_section = extra_section,
    )
}

fn show_constitution(config: &GatewayConfig) -> anyhow::Result<()> {
    let path = config.workspace_root.join(".ta/constitution.md");
    if !path.exists() {
        println!("No .ta/constitution.md found.");
        println!();
        println!("Run `ta constitution init` to draft one.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("Failed to read .ta/constitution.md: {}", e))?;

    println!("=== .ta/constitution.md ===");
    println!();
    println!("{}", content);
    Ok(())
}

fn check_constitution(config: &GatewayConfig, draft_id: Option<&str>) -> anyhow::Result<()> {
    use ta_submit::WorkflowConfig;

    let wf_path = config.workspace_root.join(".ta/workflow.toml");
    let wf_config = WorkflowConfig::load_or_default(&wf_path);

    if !wf_config.constitution.s4_scan && draft_id.is_none() {
        println!("Constitution checks:");
        println!("  s4_scan: disabled (set [constitution] s4_scan = true in .ta/workflow.toml to enable)");
        println!();
        println!("No checks configured for this project.");
        println!("More checks will be available in v0.14.1.");
        return Ok(());
    }

    println!("Constitution checks:");
    if wf_config.constitution.s4_scan {
        println!("  s4_scan: enabled — §4 inject/restore balance checked during `ta draft build`");
    }

    if let Some(id) = draft_id {
        // Run the check against a specific draft.
        let packages = super::draft::load_all_packages(config).unwrap_or_default();
        let pkg = packages.iter().find(|p| {
            p.package_id.to_string().starts_with(id)
                || p.display_id.as_deref().unwrap_or("").starts_with(id)
        });
        match pkg {
            Some(p) => {
                let goal_id_parsed = p.goal.goal_id.parse::<uuid::Uuid>().ok();
                let goal_store_res = ta_goal::GoalRunStore::new(&config.goals_dir);
                let goal_opt = goal_id_parsed
                    .zip(goal_store_res.ok())
                    .and_then(|(id, gs)| gs.get(id).ok().flatten());
                if let Some(goal) = goal_opt {
                    let warnings = super::draft::scan_s4_violations(
                        &p.changes.artifacts,
                        &goal.workspace_path,
                    );
                    if warnings.is_empty() {
                        println!("  §4 check: clean — no inject/restore imbalances found");
                    } else {
                        println!("  §4 check: {} warning(s):", warnings.len());
                        for w in &warnings {
                            println!("    {}", w.output);
                        }
                    }
                } else {
                    println!("  §4 check: could not load goal workspace — skipped");
                }
            }
            None => {
                anyhow::bail!(
                    "Draft '{}' not found. Run `ta draft list` to see available drafts.",
                    id
                );
            }
        }
    }

    Ok(())
}

fn read_file_if_exists(path: &Path) -> Option<String> {
    if path.exists() {
        std::fs::read_to_string(path).ok()
    } else {
        None
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn build_constitution_prompt_basic() {
        let dir = TempDir::new().unwrap();
        let prompt = build_constitution_prompt(dir.path(), None, None, None, None, false);
        assert!(prompt.contains("constitution.md"));
        assert!(prompt.contains("ta_ask_human"));
        assert!(prompt.contains("Core Invariants"));
    }

    #[test]
    fn build_constitution_prompt_with_plan() {
        let dir = TempDir::new().unwrap();
        let plan = "# My Project\n## Phase v0.1.0\nBootstrap.";
        let prompt = build_constitution_prompt(dir.path(), Some(plan), None, None, None, false);
        assert!(prompt.contains("PLAN.md"));
        assert!(prompt.contains("Bootstrap"));
    }

    #[test]
    fn build_constitution_prompt_non_interactive() {
        let dir = TempDir::new().unwrap();
        let prompt = build_constitution_prompt(dir.path(), None, None, None, None, true);
        assert!(prompt.contains("Non-interactive mode"));
        assert!(!prompt.contains("Ask 2-3"));
    }

    #[test]
    fn show_constitution_no_file() {
        let dir = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(dir.path());
        // Should not panic — just prints "not found" message.
        let result = show_constitution(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn show_constitution_with_file() {
        let dir = TempDir::new().unwrap();
        let ta_dir = dir.path().join(".ta");
        std::fs::create_dir_all(&ta_dir).unwrap();
        std::fs::write(
            ta_dir.join("constitution.md"),
            "# My Project Constitution\n## Core Invariants\n- Never delete main.\n",
        )
        .unwrap();

        let config = GatewayConfig::for_project(dir.path());
        let result = show_constitution(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn check_constitution_no_config() {
        let dir = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(dir.path());
        // No workflow.toml → defaults (s4_scan = false) → prints info and returns Ok.
        let result = check_constitution(&config, None);
        assert!(result.is_ok());
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let s = "a".repeat(200);
        assert_eq!(truncate(&s, 100).len(), 100);
    }
}
