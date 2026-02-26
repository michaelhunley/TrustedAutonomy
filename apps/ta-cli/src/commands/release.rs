// release.rs — Configurable release pipeline (`ta release`).
//
// Executes a YAML-defined pipeline of steps to release a new version.
// Steps can be either shell commands (`run`) or TA goal agent invocations
// (`agent`). Steps may require human approval before proceeding.
//
// Pipeline resolution order:
//   1. `.ta/release.yaml` in the project root (user override)
//   2. Built-in default pipeline (compiled into the binary)

use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Subcommand;
use serde::{Deserialize, Serialize};
use ta_mcp_gateway::GatewayConfig;

// ── CLI definition ──────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum ReleaseCommands {
    /// Run the release pipeline for a new version.
    Run {
        /// Target version (e.g., "0.4.0-alpha").
        version: String,

        /// Skip approval gates (non-interactive / CI mode).
        #[arg(long)]
        yes: bool,

        /// Dry-run: show what would be executed without running anything.
        #[arg(long)]
        dry_run: bool,

        /// Start from a specific step (1-indexed). Skips earlier steps.
        #[arg(long)]
        from_step: Option<usize>,

        /// Custom pipeline file (overrides default resolution).
        #[arg(long)]
        pipeline: Option<PathBuf>,
    },
    /// Show the pipeline that would be executed (without running it).
    Show {
        /// Custom pipeline file (overrides default resolution).
        #[arg(long)]
        pipeline: Option<PathBuf>,
    },
    /// Initialize a `.ta/release.yaml` in the project from the default template.
    Init,
}

pub fn execute(cmd: &ReleaseCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match cmd {
        ReleaseCommands::Run {
            version,
            yes,
            dry_run,
            from_step,
            pipeline,
        } => run_pipeline(
            config,
            version,
            *yes,
            *dry_run,
            *from_step,
            pipeline.as_deref(),
        ),
        ReleaseCommands::Show { pipeline } => show_pipeline(config, pipeline.as_deref()),
        ReleaseCommands::Init => init_pipeline(config),
    }
}

// ── Pipeline data model ─────────────────────────────────────────

/// A release pipeline loaded from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleasePipeline {
    /// Human-readable pipeline name.
    #[serde(default = "default_pipeline_name")]
    pub name: String,
    /// Ordered list of pipeline steps.
    pub steps: Vec<PipelineStep>,
}

fn default_pipeline_name() -> String {
    "release".to_string()
}

/// A single step in the release pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    /// Human-readable step name.
    pub name: String,

    /// Shell command to run (mutually exclusive with `agent`).
    #[serde(default)]
    pub run: Option<String>,

    /// TA agent to invoke (mutually exclusive with `run`).
    #[serde(default)]
    pub agent: Option<AgentStep>,

    /// Objective/description for context (used by agent steps and display).
    #[serde(default)]
    pub objective: Option<String>,

    /// If true, pause for human approval before this step executes.
    #[serde(default)]
    pub requires_approval: bool,

    /// Expected output artifact path (informational).
    #[serde(default)]
    pub output: Option<String>,

    /// Working directory override (relative to project root).
    #[serde(default)]
    pub working_dir: Option<String>,

    /// Environment variables for this step.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// Configuration for an agent-driven pipeline step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    /// Agent system to use (e.g., "claude-code").
    #[serde(default = "default_agent_id")]
    pub id: String,

    /// Phase to associate with the goal.
    #[serde(default)]
    pub phase: Option<String>,
}

fn default_agent_id() -> String {
    "claude-code".to_string()
}

impl PipelineStep {
    fn validate(&self) -> anyhow::Result<()> {
        if self.run.is_none() && self.agent.is_none() {
            anyhow::bail!(
                "Step '{}': must have either 'run' or 'agent' defined",
                self.name
            );
        }
        if self.run.is_some() && self.agent.is_some() {
            anyhow::bail!(
                "Step '{}': cannot have both 'run' and 'agent' — pick one",
                self.name
            );
        }
        Ok(())
    }
}

// ── Pipeline resolution ─────────────────────────────────────────

/// Load the release pipeline, checking for a user override first.
fn load_pipeline(
    config: &GatewayConfig,
    override_path: Option<&Path>,
) -> anyhow::Result<ReleasePipeline> {
    // 1. Explicit override from --pipeline flag.
    if let Some(path) = override_path {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            anyhow::anyhow!("Cannot read pipeline file '{}': {}", path.display(), e)
        })?;
        let pipeline: ReleasePipeline = serde_yaml::from_str(&contents)?;
        validate_pipeline(&pipeline)?;
        return Ok(pipeline);
    }

    // 2. Project-local override: .ta/release.yaml
    let project_yaml = config.workspace_root.join(".ta").join("release.yaml");
    if project_yaml.exists() {
        let contents = std::fs::read_to_string(&project_yaml)?;
        let pipeline: ReleasePipeline = serde_yaml::from_str(&contents)?;
        validate_pipeline(&pipeline)?;
        return Ok(pipeline);
    }

    // 3. Built-in default pipeline.
    let pipeline: ReleasePipeline = serde_yaml::from_str(DEFAULT_PIPELINE_YAML)?;
    validate_pipeline(&pipeline)?;
    Ok(pipeline)
}

fn validate_pipeline(pipeline: &ReleasePipeline) -> anyhow::Result<()> {
    if pipeline.steps.is_empty() {
        anyhow::bail!("Pipeline has no steps");
    }
    for step in &pipeline.steps {
        step.validate()?;
    }
    Ok(())
}

// ── Commit collection ───────────────────────────────────────────

/// Collect commit messages since the last git tag.
fn collect_commits_since_last_tag(project_root: &Path) -> anyhow::Result<(String, Option<String>)> {
    // Find last tag.
    let last_tag_output = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .current_dir(project_root)
        .output();

    let last_tag = match last_tag_output {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        _ => None,
    };

    let log_args = match &last_tag {
        Some(tag) => vec![
            "log".to_string(),
            format!("{}..HEAD", tag),
            "--pretty=format:%s".to_string(),
            "--no-merges".to_string(),
        ],
        None => vec![
            "log".to_string(),
            "--pretty=format:%s".to_string(),
            "--no-merges".to_string(),
        ],
    };

    let output = Command::new("git")
        .args(&log_args)
        .current_dir(project_root)
        .output()?;

    let commits = String::from_utf8_lossy(&output.stdout).to_string();
    Ok((commits, last_tag))
}

// ── Variable substitution ───────────────────────────────────────

/// Substitute `${VERSION}`, `${TAG}`, `${COMMITS}`, `${LAST_TAG}` in a string.
fn substitute_vars(template: &str, version: &str, commits: &str, last_tag: Option<&str>) -> String {
    template
        .replace("${VERSION}", version)
        .replace("${TAG}", &format!("v{}", version))
        .replace("${COMMITS}", commits)
        .replace("${LAST_TAG}", last_tag.unwrap_or(""))
}

// ── Pipeline execution ──────────────────────────────────────────

fn run_pipeline(
    config: &GatewayConfig,
    version: &str,
    skip_approvals: bool,
    dry_run: bool,
    from_step: Option<usize>,
    pipeline_path: Option<&Path>,
) -> anyhow::Result<()> {
    // Validate version format.
    let version_re = regex::Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$")?;
    if !version_re.is_match(version) {
        anyhow::bail!(
            "Invalid version '{}'. Expected semver (e.g., 0.4.0-alpha, 1.0.0)",
            version
        );
    }

    let pipeline = load_pipeline(config, pipeline_path)?;
    let (commits, last_tag) = collect_commits_since_last_tag(&config.workspace_root)?;

    let total = pipeline.steps.len();
    let start_idx = from_step.map(|s| s.saturating_sub(1)).unwrap_or(0);

    println!("Release pipeline: {}", pipeline.name);
    println!("Target version:   v{}", version);
    println!(
        "Steps:            {} (starting from {})",
        total,
        start_idx + 1
    );
    if dry_run {
        println!("Mode:             DRY RUN");
    }
    println!();

    for (i, step) in pipeline.steps.iter().enumerate() {
        if i < start_idx {
            println!(
                "[{}/{}] {} — skipped (--from-step)",
                i + 1,
                total,
                step.name
            );
            continue;
        }

        // Approval gate.
        if step.requires_approval && !skip_approvals && !dry_run {
            println!("[{}/{}] {} — requires approval", i + 1, total, step.name);
            if !prompt_approval(&step.name)? {
                println!("Aborted at step {}.", i + 1);
                return Ok(());
            }
        }

        println!("[{}/{}] {} ...", i + 1, total, step.name);

        if dry_run {
            print_step_dry_run(step, version, &commits, last_tag.as_deref());
            continue;
        }

        if let Some(ref cmd_template) = step.run {
            execute_shell_step(
                config,
                step,
                cmd_template,
                version,
                &commits,
                last_tag.as_deref(),
            )?;
        } else if let Some(ref agent) = step.agent {
            execute_agent_step(config, step, agent, version, &commits, last_tag.as_deref())?;
        }

        println!("[{}/{}] {} — done", i + 1, total, step.name);
        println!();
    }

    println!("Release pipeline complete.");
    Ok(())
}

fn execute_shell_step(
    config: &GatewayConfig,
    step: &PipelineStep,
    cmd_template: &str,
    version: &str,
    commits: &str,
    last_tag: Option<&str>,
) -> anyhow::Result<()> {
    let cmd = substitute_vars(cmd_template, version, commits, last_tag);

    let work_dir = match &step.working_dir {
        Some(d) => config.workspace_root.join(d),
        None => config.workspace_root.clone(),
    };

    let mut command = Command::new("sh");
    command.arg("-c").arg(&cmd).current_dir(&work_dir);

    // Inject step-level env vars with substitution.
    for (k, v) in &step.env {
        command.env(k, substitute_vars(v, version, commits, last_tag));
    }

    let status = command.status()?;
    if !status.success() {
        anyhow::bail!(
            "Step '{}' failed (exit code: {:?})",
            step.name,
            status.code()
        );
    }
    Ok(())
}

fn execute_agent_step(
    config: &GatewayConfig,
    step: &PipelineStep,
    agent: &AgentStep,
    version: &str,
    commits: &str,
    last_tag: Option<&str>,
) -> anyhow::Result<()> {
    let objective = step.objective.as_deref().unwrap_or("Execute release step");
    let objective = substitute_vars(objective, version, commits, last_tag);

    let title = format!("release: {}", step.name);

    // Build the ta run command.
    let mut args = vec![
        "run".to_string(),
        title,
        "--agent".to_string(),
        agent.id.clone(),
        "--source".to_string(),
        config.workspace_root.display().to_string(),
        "--objective".to_string(),
        objective,
    ];

    if let Some(ref phase) = agent.phase {
        args.push("--phase".to_string());
        args.push(phase.clone());
    }

    // Resolve the `ta` binary path (same binary we're running from).
    let ta_bin = std::env::current_exe()?;
    let status = Command::new(&ta_bin)
        .args(&args)
        .current_dir(&config.workspace_root)
        .status()?;

    if !status.success() {
        anyhow::bail!(
            "Agent step '{}' failed (exit code: {:?})",
            step.name,
            status.code()
        );
    }

    // Auto-approve and apply the draft so output files land in the working
    // directory before the next pipeline step runs. Without this, agent output
    // stays in staging and subsequent shell steps can't find it.
    println!("  Auto-applying agent draft...");
    let latest_draft = find_latest_draft(config)?;
    if let Some(draft_id) = latest_draft {
        let id_str = draft_id.to_string();

        // Approve.
        let approve_status = Command::new(&ta_bin)
            .args([
                "draft",
                "approve",
                &id_str,
                "--reviewer",
                "release-pipeline",
            ])
            .current_dir(&config.workspace_root)
            .status()?;
        if !approve_status.success() {
            anyhow::bail!(
                "Failed to auto-approve draft {} for agent step '{}'",
                id_str,
                step.name
            );
        }

        // Apply (no git commit — the release pipeline handles commits itself).
        let apply_status = Command::new(&ta_bin)
            .args(["draft", "apply", &id_str])
            .current_dir(&config.workspace_root)
            .status()?;
        if !apply_status.success() {
            anyhow::bail!(
                "Failed to auto-apply draft {} for agent step '{}'",
                id_str,
                step.name
            );
        }
        println!("  Draft {} applied to working directory.", id_str);
    } else {
        println!(
            "  Warning: no draft found after agent step '{}'.",
            step.name
        );
    }

    Ok(())
}

/// Find the most recently created draft package ID.
fn find_latest_draft(config: &GatewayConfig) -> anyhow::Result<Option<uuid::Uuid>> {
    let dir = &config.pr_packages_dir;
    if !dir.exists() {
        return Ok(None);
    }

    let mut newest: Option<(std::time::SystemTime, uuid::Uuid)> = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(id) = uuid::Uuid::parse_str(stem) {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if newest.as_ref().is_none_or(|(t, _)| modified > *t) {
                                newest = Some((modified, id));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(newest.map(|(_, id)| id))
}

fn print_step_dry_run(step: &PipelineStep, version: &str, commits: &str, last_tag: Option<&str>) {
    if let Some(ref cmd) = step.run {
        let resolved = substitute_vars(cmd, version, commits, last_tag);
        println!("  type: shell");
        println!("  command: {}", resolved);
    } else if let Some(ref agent) = step.agent {
        println!("  type: agent ({})", agent.id);
        if let Some(ref obj) = step.objective {
            println!(
                "  objective: {}",
                substitute_vars(obj, version, commits, last_tag)
            );
        }
    }
    if step.requires_approval {
        println!("  approval: required");
    }
    if let Some(ref out) = step.output {
        println!("  output: {}", out);
    }
    println!();
}

fn prompt_approval(step_name: &str) -> anyhow::Result<bool> {
    use std::io::{self, Write};
    print!("Proceed with '{}'? [y/N] ", step_name);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

// ── Show pipeline ───────────────────────────────────────────────

fn show_pipeline(config: &GatewayConfig, pipeline_path: Option<&Path>) -> anyhow::Result<()> {
    let pipeline = load_pipeline(config, pipeline_path)?;

    println!("Pipeline: {}", pipeline.name);
    println!("Steps:    {}", pipeline.steps.len());
    println!();

    for (i, step) in pipeline.steps.iter().enumerate() {
        let kind = if step.run.is_some() {
            "shell"
        } else if step.agent.is_some() {
            "agent"
        } else {
            "unknown"
        };
        let approval = if step.requires_approval {
            " [approval required]"
        } else {
            ""
        };
        println!("  {}. {} ({}){}", i + 1, step.name, kind, approval);

        if let Some(ref obj) = step.objective {
            println!("     {}", obj);
        }
    }
    Ok(())
}

// ── Init pipeline ───────────────────────────────────────────────

fn init_pipeline(config: &GatewayConfig) -> anyhow::Result<()> {
    let ta_dir = config.workspace_root.join(".ta");
    std::fs::create_dir_all(&ta_dir)?;

    let dest = ta_dir.join("release.yaml");
    if dest.exists() {
        anyhow::bail!(
            "Pipeline already exists at {}. Delete it first to re-initialize.",
            dest.display()
        );
    }

    std::fs::write(&dest, DEFAULT_PIPELINE_YAML)?;
    println!("Created {}", dest.display());
    println!("Edit this file to customize your release pipeline.");
    Ok(())
}

// ── Default built-in pipeline ───────────────────────────────────

const DEFAULT_PIPELINE_YAML: &str = r#"# .ta/release.yaml — TA release pipeline configuration.
#
# Variables available in all string fields:
#   ${VERSION}  — target version (e.g., "0.4.0-alpha")
#   ${TAG}      — git tag (e.g., "v0.4.0-alpha")
#   ${COMMITS}  — newline-separated commit messages since last tag
#   ${LAST_TAG} — previous git tag (empty if none)
#
# Each step must have either `run` (shell command) or `agent` (TA goal).
# Steps with `requires_approval: true` pause for human confirmation.

name: ta-release

steps:
  - name: Preflight checks
    run: |
      set -e
      if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "ERROR: Working tree is not clean. Commit or stash changes first."
        exit 1
      fi
      if git rev-parse "${TAG}" >/dev/null 2>&1; then
        echo "ERROR: Tag '${TAG}' already exists."
        exit 1
      fi
      echo "Preflight OK — clean tree, tag ${TAG} is available."

  - name: Version bump
    run: |
      set -e
      # Bump all Cargo.toml versions
      for f in Cargo.toml crates/*/Cargo.toml apps/*/Cargo.toml; do
        [ -f "$f" ] && sed -i.bak '/^\[package\]/,/^\[/s/^version = ".*"/version = "'"${VERSION}"'"/' "$f" && rm -f "${f}.bak"
      done
      # Update DISCLAIMER.md if it exists
      [ -f DISCLAIMER.md ] && sed -i.bak 's/^\*\*Version\*\*: .*/\*\*Version\*\*: '"${VERSION}"'/' DISCLAIMER.md && rm -f DISCLAIMER.md.bak
      echo "Versions bumped to ${VERSION}."

  - name: Build & verify
    run: |
      set -e
      ./dev cargo build --workspace
      ./dev cargo test --workspace
      ./dev cargo clippy --workspace --all-targets -- -D warnings
      ./dev cargo fmt --all -- --check
      echo "All checks passed."

  - name: Generate release notes
    agent:
      id: claude-code
    objective: |
      Synthesize user-facing release notes for version ${TAG}.
      Commits since ${LAST_TAG}:
      ${COMMITS}

      Write the notes to .release-draft.md in this format:
      ## ${TAG}
      ### New Features
      - ...
      ### Improvements
      - ...
      ### Bug Fixes
      - ...
    output: .release-draft.md

  - name: Review release notes
    requires_approval: true
    run: |
      if [ -f .release-draft.md ]; then
        echo "── Release notes draft ──"
        cat .release-draft.md
      else
        echo "No .release-draft.md found. Skipping."
      fi

  - name: Commit and tag
    run: |
      set -e
      git add -A
      git commit -m "Release ${TAG}

      Bump all crate versions to ${VERSION}."
      git tag -a "${TAG}" -m "Release ${TAG}"
      echo "Created commit and tag ${TAG}."

  - name: Push
    requires_approval: true
    run: |
      set -e
      BRANCH="$(git branch --show-current)"
      git push origin "$BRANCH"
      git push origin "${TAG}"
      echo "Pushed ${TAG}. GitHub Actions will build the release."
"#;

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_default_pipeline() {
        let pipeline: ReleasePipeline = serde_yaml::from_str(DEFAULT_PIPELINE_YAML).unwrap();
        assert_eq!(pipeline.name, "ta-release");
        assert!(!pipeline.steps.is_empty());
        // Validate all steps.
        for step in &pipeline.steps {
            step.validate().unwrap();
        }
    }

    #[test]
    fn step_validation_requires_run_or_agent() {
        let step = PipelineStep {
            name: "bad".to_string(),
            run: None,
            agent: None,
            objective: None,
            requires_approval: false,
            output: None,
            working_dir: None,
            env: Default::default(),
        };
        assert!(step.validate().is_err());
    }

    #[test]
    fn step_validation_rejects_both_run_and_agent() {
        let step = PipelineStep {
            name: "bad".to_string(),
            run: Some("echo hi".to_string()),
            agent: Some(AgentStep {
                id: "claude-code".to_string(),
                phase: None,
            }),
            objective: None,
            requires_approval: false,
            output: None,
            working_dir: None,
            env: Default::default(),
        };
        assert!(step.validate().is_err());
    }

    #[test]
    fn variable_substitution() {
        let result = substitute_vars(
            "Build ${TAG} from ${VERSION}, last was ${LAST_TAG}",
            "1.0.0",
            "commit1\ncommit2",
            Some("v0.9.0"),
        );
        assert_eq!(result, "Build v1.0.0 from 1.0.0, last was v0.9.0");
    }

    #[test]
    fn variable_substitution_no_last_tag() {
        let result = substitute_vars("since ${LAST_TAG} end", "1.0.0", "", None);
        assert_eq!(result, "since  end");
    }

    #[test]
    fn load_custom_pipeline_yaml() {
        let yaml = r#"
name: custom
steps:
  - name: greet
    run: echo hello
  - name: agent step
    agent:
      id: codex
      phase: "4b"
    objective: Do something
    requires_approval: true
"#;
        let pipeline: ReleasePipeline = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(pipeline.name, "custom");
        assert_eq!(pipeline.steps.len(), 2);
        assert_eq!(pipeline.steps[0].run.as_deref(), Some("echo hello"));
        assert!(pipeline.steps[0].agent.is_none());
        assert!(pipeline.steps[1].run.is_none());
        assert_eq!(pipeline.steps[1].agent.as_ref().unwrap().id, "codex");
        assert_eq!(
            pipeline.steps[1].agent.as_ref().unwrap().phase.as_deref(),
            Some("4b")
        );
        assert!(pipeline.steps[1].requires_approval);
    }

    #[test]
    fn init_creates_release_yaml() {
        let temp = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(temp.path());
        init_pipeline(&config).unwrap();

        let path = temp.path().join(".ta").join("release.yaml");
        assert!(path.exists());

        // Should not overwrite existing.
        assert!(init_pipeline(&config).is_err());
    }

    #[test]
    fn load_pipeline_uses_project_override() {
        let temp = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(temp.path());

        // Without override, uses built-in.
        let p = load_pipeline(&config, None).unwrap();
        assert_eq!(p.name, "ta-release");

        // Write a project override.
        let ta_dir = temp.path().join(".ta");
        std::fs::create_dir_all(&ta_dir).unwrap();
        std::fs::write(
            ta_dir.join("release.yaml"),
            "name: my-project\nsteps:\n  - name: test\n    run: echo ok\n",
        )
        .unwrap();

        let p = load_pipeline(&config, None).unwrap();
        assert_eq!(p.name, "my-project");
    }

    #[test]
    fn load_pipeline_explicit_path() {
        let temp = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(temp.path());

        let custom = temp.path().join("custom.yaml");
        std::fs::write(
            &custom,
            "name: explicit\nsteps:\n  - name: x\n    run: echo x\n",
        )
        .unwrap();

        let p = load_pipeline(&config, Some(&custom)).unwrap();
        assert_eq!(p.name, "explicit");
    }

    #[test]
    fn version_validation() {
        let re = regex::Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$").unwrap();
        assert!(re.is_match("0.4.0-alpha"));
        assert!(re.is_match("1.0.0"));
        assert!(re.is_match("0.1.0-rc.1"));
        assert!(!re.is_match("v0.4.0"));
        assert!(!re.is_match("bad"));
        assert!(!re.is_match("1.0"));
    }

    #[test]
    fn show_pipeline_output() {
        let temp = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(temp.path());
        // Just ensure it doesn't panic.
        show_pipeline(&config, None).unwrap();
    }

    #[test]
    fn validate_empty_pipeline_fails() {
        let pipeline = ReleasePipeline {
            name: "empty".to_string(),
            steps: vec![],
        };
        assert!(validate_pipeline(&pipeline).is_err());
    }

    #[test]
    fn agent_step_default_id() {
        let yaml = "id: claude-code";
        let agent: AgentStep = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(agent.id, "claude-code");
        assert!(agent.phase.is_none());
    }

    #[test]
    fn pipeline_step_env_vars() {
        let yaml = r#"
name: with-env
steps:
  - name: test
    run: echo $MY_VAR
    env:
      MY_VAR: "value-${VERSION}"
"#;
        let pipeline: ReleasePipeline = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            pipeline.steps[0].env.get("MY_VAR").unwrap(),
            "value-${VERSION}"
        );
    }

    #[test]
    fn dry_run_does_not_execute() {
        let temp = TempDir::new().unwrap();
        // Initialize a git repo so commit collection doesn't fail.
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let config = GatewayConfig::for_project(temp.path());

        // Write a pipeline that would fail if actually run.
        let ta_dir = temp.path().join(".ta");
        std::fs::create_dir_all(&ta_dir).unwrap();
        std::fs::write(
            ta_dir.join("release.yaml"),
            "name: test\nsteps:\n  - name: fail\n    run: exit 1\n",
        )
        .unwrap();

        // Dry run should succeed even though the step would fail.
        run_pipeline(&config, "1.0.0", true, true, None, None).unwrap();
    }
}
