// verify.rs — Pre-draft verification gate (v0.10.8).
//
// Runs configurable build/lint/test checks in a staging directory.
// Used by `ta run` (after agent exit, before draft build) and
// by `ta verify` (standalone manual verification).

use std::path::Path;
use std::time::Duration;

use ta_changeset::draft_package::VerificationWarning;
use ta_goal::GoalRunStore;
use ta_mcp_gateway::GatewayConfig;
use ta_submit::config::VerifyConfig;

/// Result of running verification commands.
#[derive(Debug)]
pub struct VerificationResult {
    /// Whether all commands passed.
    pub passed: bool,
    /// Warnings for commands that failed (populated regardless of on_failure mode).
    pub warnings: Vec<VerificationWarning>,
}

/// Run verification commands in the given directory.
///
/// Returns a `VerificationResult` with pass/fail status and any warnings.
/// The caller decides what to do based on `on_failure` mode.
pub fn run_verification(config: &VerifyConfig, staging_dir: &Path) -> VerificationResult {
    if config.commands.is_empty() {
        return VerificationResult {
            passed: true,
            warnings: Vec::new(),
        };
    }

    println!();
    println!(
        "Running pre-draft verification ({} commands)...",
        config.commands.len()
    );

    let timeout = Duration::from_secs(config.timeout);
    let mut warnings = Vec::new();
    let mut all_passed = true;

    for (i, cmd) in config.commands.iter().enumerate() {
        println!("  [{}/{}] {}", i + 1, config.commands.len(), cmd);

        match run_single_command(cmd, staging_dir, timeout) {
            Ok(output) => {
                if output.success {
                    println!("        PASS");
                } else {
                    all_passed = false;
                    println!(
                        "        FAIL (exit code: {})",
                        output.exit_code.unwrap_or(-1)
                    );

                    // Truncate output to 2000 chars for storage.
                    let truncated_output = if output.combined_output.len() > 2000 {
                        format!(
                            "{}... (truncated, {} bytes total)",
                            &output.combined_output[..2000],
                            output.combined_output.len()
                        )
                    } else {
                        output.combined_output
                    };

                    warnings.push(VerificationWarning {
                        command: cmd.clone(),
                        exit_code: output.exit_code,
                        output: truncated_output,
                    });
                }
            }
            Err(e) => {
                all_passed = false;
                println!("        ERROR: {}", e);
                warnings.push(VerificationWarning {
                    command: cmd.clone(),
                    exit_code: None,
                    output: e.to_string(),
                });
            }
        }
    }

    if all_passed {
        println!("  All verification checks passed.");
    } else {
        println!(
            "  {} of {} verification checks failed.",
            warnings.len(),
            config.commands.len()
        );
    }

    VerificationResult {
        passed: all_passed,
        warnings,
    }
}

/// Output from running a single verification command.
struct CommandOutput {
    success: bool,
    exit_code: Option<i32>,
    combined_output: String,
}

/// Run a single shell command in the given directory with a timeout.
fn run_single_command(
    cmd: &str,
    working_dir: &Path,
    timeout: Duration,
) -> anyhow::Result<CommandOutput> {
    use std::process::{Command, Stdio};

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn '{}': {}", cmd, e))?;

    // Wait with timeout.
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout_buf = String::new();
                let mut stderr_buf = String::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = std::io::Read::read_to_string(&mut s, &mut stdout_buf);
                }
                if let Some(mut s) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut s, &mut stderr_buf);
                }

                let combined = if stderr_buf.is_empty() {
                    stdout_buf
                } else if stdout_buf.is_empty() {
                    stderr_buf
                } else {
                    format!("{}\n{}", stdout_buf, stderr_buf)
                };

                return Ok(CommandOutput {
                    success: status.success(),
                    exit_code: status.code(),
                    combined_output: combined,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(anyhow::anyhow!(
                        "Command timed out after {}s: {}",
                        timeout.as_secs(),
                        cmd
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to wait for '{}': {}", cmd, e));
            }
        }
    }
}

/// `ta verify` standalone command — run verification against a goal's staging directory.
pub fn execute(config: &GatewayConfig, goal_id: Option<&str>) -> anyhow::Result<()> {
    let goal_store = GoalRunStore::new(&config.goals_dir)?;

    // Find the goal.
    let goal = if let Some(id_prefix) = goal_id {
        // Try UUID parse first.
        if let Ok(uuid) = uuid::Uuid::parse_str(id_prefix) {
            goal_store
                .get(uuid)?
                .ok_or_else(|| anyhow::anyhow!("Goal {} not found", id_prefix))?
        } else {
            // Prefix match.
            let goals = goal_store.list()?;
            let matches: Vec<_> = goals
                .into_iter()
                .filter(|g| g.goal_run_id.to_string().starts_with(id_prefix))
                .collect();
            match matches.len() {
                0 => anyhow::bail!("No goal found matching '{}'", id_prefix),
                1 => matches.into_iter().next().unwrap(),
                n => anyhow::bail!(
                    "Ambiguous prefix '{}' matches {} goals. Use a longer prefix.",
                    id_prefix,
                    n
                ),
            }
        }
    } else {
        // Find the most recent running or pr-ready goal.
        let mut goals = goal_store.list()?;
        goals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        goals
            .into_iter()
            .find(|g| {
                matches!(
                    g.state,
                    ta_goal::GoalRunState::Running | ta_goal::GoalRunState::PrReady
                )
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No active goal found. Specify a goal ID, or start a goal with `ta run`."
                )
            })?
    };

    let staging_dir = &goal.workspace_path;
    if !staging_dir.exists() {
        anyhow::bail!(
            "Staging directory does not exist: {}\nThe goal's workspace may have been cleaned up.",
            staging_dir.display()
        );
    }

    // Load verify config from the staging directory's workflow.toml.
    let workflow_toml = staging_dir.join(".ta/workflow.toml");
    let workflow_config = ta_submit::WorkflowConfig::load_or_default(&workflow_toml);

    if workflow_config.verify.commands.is_empty() {
        println!("No verification commands configured.");
        println!();
        println!("Add a [verify] section to .ta/workflow.toml:");
        println!("  [verify]");
        println!("  commands = [\"cargo test --workspace\"]");
        return Ok(());
    }

    println!(
        "Verifying goal: {} ({})",
        goal.title,
        &goal.goal_run_id.to_string()[..8]
    );
    println!("  Staging: {}", staging_dir.display());

    let result = run_verification(&workflow_config.verify, staging_dir);

    if result.passed {
        println!();
        println!("All verification checks passed. Ready to build draft.");
    } else {
        println!();
        for warning in &result.warnings {
            println!("Failed: {}", warning.command);
            if !warning.output.is_empty() {
                // Show first 10 lines of output.
                for line in warning.output.lines().take(10) {
                    println!("  {}", line);
                }
                let line_count = warning.output.lines().count();
                if line_count > 10 {
                    println!("  ... ({} more lines)", line_count - 10);
                }
            }
            println!();
        }
        println!("Fix the issues above, then re-run `ta verify`.");
        println!("Or use `ta run --follow-up` to re-enter the agent.");
    }

    if result.passed {
        Ok(())
    } else {
        // Exit with error to signal failure in scripts.
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_submit::config::VerifyOnFailure;
    use tempfile::TempDir;

    #[test]
    fn empty_commands_passes() {
        let config = VerifyConfig::default();
        let dir = TempDir::new().unwrap();
        let result = run_verification(&config, dir.path());
        assert!(result.passed);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn passing_command() {
        let config = VerifyConfig {
            commands: vec!["true".to_string()],
            on_failure: VerifyOnFailure::Block,
            timeout: 30,
        };
        let dir = TempDir::new().unwrap();
        let result = run_verification(&config, dir.path());
        assert!(result.passed);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn failing_command() {
        let config = VerifyConfig {
            commands: vec!["false".to_string()],
            on_failure: VerifyOnFailure::Block,
            timeout: 30,
        };
        let dir = TempDir::new().unwrap();
        let result = run_verification(&config, dir.path());
        assert!(!result.passed);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].command, "false");
    }

    #[test]
    fn mixed_commands_reports_only_failures() {
        let config = VerifyConfig {
            commands: vec!["true".to_string(), "false".to_string(), "true".to_string()],
            on_failure: VerifyOnFailure::Block,
            timeout: 30,
        };
        let dir = TempDir::new().unwrap();
        let result = run_verification(&config, dir.path());
        assert!(!result.passed);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].command, "false");
    }

    #[test]
    fn command_output_captured() {
        let config = VerifyConfig {
            commands: vec!["echo 'hello world' && exit 1".to_string()],
            on_failure: VerifyOnFailure::Block,
            timeout: 30,
        };
        let dir = TempDir::new().unwrap();
        let result = run_verification(&config, dir.path());
        assert!(!result.passed);
        assert!(result.warnings[0].output.contains("hello world"));
    }

    #[test]
    fn timeout_produces_warning() {
        let config = VerifyConfig {
            commands: vec!["sleep 10".to_string()],
            on_failure: VerifyOnFailure::Block,
            timeout: 1,
        };
        let dir = TempDir::new().unwrap();
        let result = run_verification(&config, dir.path());
        assert!(!result.passed);
        assert!(result.warnings[0].output.contains("timed out"));
    }
}
