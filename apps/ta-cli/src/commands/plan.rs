// plan.rs — Plan tracking: parse PLAN.md, show status, update phases.
//
// The canonical project plan lives in PLAN.md at the project root.
// Each phase has a machine-parseable status marker:
//   ## Phase 4b — Per-Artifact Review Model
//   <!-- status: pending -->
//
// Sub-phases use ### headers with the same status marker pattern:
//   ### v0.3.1 — Plan Lifecycle Automation
//   <!-- status: pending -->
//
// `ta plan list` shows all phases with their status.
// `ta plan status` shows a summary of progress.
// `ta plan next` shows the next pending phase and optionally creates a goal for it.
// `ta plan history` shows plan change history.
// `ta pr apply` auto-updates PLAN.md when a goal with --phase completes.

use std::fmt;
use std::path::Path;

use clap::Subcommand;
use ta_mcp_gateway::GatewayConfig;

#[derive(Subcommand)]
pub enum PlanCommands {
    /// List all plan phases with their status.
    List,
    /// Show a summary of plan progress.
    Status,
    /// Show the next pending phase and suggest creating a goal for it.
    Next,
    /// Show plan change history (status transitions recorded in .ta/plan_history.jsonl).
    History,
    /// Validate completed work against the plan for a given phase.
    Validate {
        /// Phase ID to validate (e.g., "v0.3.1").
        phase: String,
    },
}

pub fn execute(cmd: &PlanCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match cmd {
        PlanCommands::List => list_phases(config),
        PlanCommands::Status => show_status(config),
        PlanCommands::Next => show_next(config),
        PlanCommands::History => show_history(config),
        PlanCommands::Validate { phase } => validate_phase(config, phase),
    }
}

// ── Data model ───────────────────────────────────────────────────

/// Status of a plan phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStatus {
    Pending,
    InProgress,
    Done,
}

impl fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanStatus::Pending => write!(f, "pending"),
            PlanStatus::InProgress => write!(f, "in_progress"),
            PlanStatus::Done => write!(f, "done"),
        }
    }
}

/// A parsed plan phase from PLAN.md.
#[derive(Debug, Clone)]
pub struct PlanPhase {
    /// Phase identifier (e.g., "0", "4b", "4a.1").
    pub id: String,
    /// Human-readable title (e.g., "Per-Artifact Review Model").
    pub title: String,
    /// Current status.
    pub status: PlanStatus,
}

// ── Parsing ──────────────────────────────────────────────────────

/// Parse PLAN.md content into a list of phases.
///
/// Supports two header formats:
///   ## Phase 4b — Per-Artifact Review Model     (top-level phases)
///   ### v0.3.1 — Plan Lifecycle Automation      (sub-phases under release headers)
///
/// Both expect a `<!-- status: pending -->` marker on the next line.
pub fn parse_plan(content: &str) -> Vec<PlanPhase> {
    let mut phases = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        // Match top-level phase headers: ## Phase <id> — <title>
        // Support both em dash (—) and regular dash (-).
        if let Some(rest) = line
            .strip_prefix("## Phase ")
            .or_else(|| line.strip_prefix("## Phase\u{a0}"))
        {
            let (id, title) = split_phase_header(rest);
            let status = if i + 1 < lines.len() {
                parse_status_marker(lines[i + 1])
            } else {
                PlanStatus::Pending
            };
            phases.push(PlanPhase { id, title, status });
        }
        // Match sub-phase headers: ### v0.X.Y — Title
        // These are versioned sub-phases under release group headers (## v0.X).
        else if let Some(rest) = line.strip_prefix("### v") {
            // Re-prepend "v" for the ID.
            let full = format!("v{}", rest);
            let (id, title) = split_phase_header(&full);
            let status = if i + 1 < lines.len() {
                parse_status_marker(lines[i + 1])
            } else {
                PlanStatus::Pending
            };
            phases.push(PlanPhase { id, title, status });
        }

        i += 1;
    }

    phases
}

/// Split a phase header into (id, title) on em-dash or space-dash-space.
fn split_phase_header(rest: &str) -> (String, String) {
    let (id, title) = if let Some(pos) = rest.find(" — ") {
        (&rest[..pos], rest[pos + " — ".len()..].trim())
    } else if let Some(pos) = rest.find(" - ") {
        (&rest[..pos], rest[pos + " - ".len()..].trim())
    } else {
        (rest.trim(), "")
    };
    (id.trim().to_string(), title.to_string())
}

/// Parse a status marker comment: `<!-- status: done -->`.
fn parse_status_marker(line: &str) -> PlanStatus {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("<!-- status:") {
        if let Some(status_str) = rest.strip_suffix("-->") {
            return match status_str.trim() {
                "done" => PlanStatus::Done,
                "in_progress" => PlanStatus::InProgress,
                _ => PlanStatus::Pending,
            };
        }
    }
    PlanStatus::Pending
}

/// Update a phase's status in PLAN.md content. Returns the new content.
///
/// Finds the phase by ID (supports both `## Phase <id>` and `### <id>` headers)
/// and replaces its status marker.
pub fn update_phase_status(content: &str, phase_id: &str, new_status: PlanStatus) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::with_capacity(lines.len());

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Check if this is the target phase header.
        let is_target = extract_phase_id_from_header(trimmed)
            .map(|id| id == phase_id)
            .unwrap_or(false);

        result.push(line.to_string());

        // If this is the target phase, replace the next line's status marker.
        if is_target && i + 1 < lines.len() {
            let next_line = lines[i + 1].trim();
            if next_line.starts_with("<!-- status:") {
                result.push(format!("<!-- status: {} -->", new_status));
                i += 2; // Skip the old status line.
                continue;
            }
        }

        i += 1;
    }

    result.join("\n")
}

/// Extract phase ID from a header line, supporting both formats:
/// - `## Phase <id> — <title>` → Some("<id>")
/// - `### v0.3.1 — <title>` → Some("v0.3.1")
/// - anything else → None
fn extract_phase_id_from_header(trimmed: &str) -> Option<String> {
    // ## Phase <id> — <title>
    if let Some(rest) = trimmed
        .strip_prefix("## Phase ")
        .or_else(|| trimmed.strip_prefix("## Phase\u{a0}"))
    {
        let id = if let Some(pos) = rest.find(" — ") {
            rest[..pos].trim()
        } else if let Some(pos) = rest.find(" - ") {
            rest[..pos].trim()
        } else {
            rest.trim()
        };
        return Some(id.to_string());
    }
    // ### v0.X.Y — <title>
    if let Some(rest) = trimmed.strip_prefix("### v") {
        let full = format!("v{}", rest);
        let id = if let Some(pos) = full.find(" — ") {
            full[..pos].trim()
        } else if let Some(pos) = full.find(" - ") {
            full[..pos].trim()
        } else {
            full.trim()
        };
        return Some(id.to_string());
    }
    None
}

/// Read and parse PLAN.md from a project directory.
pub fn load_plan(project_root: &Path) -> anyhow::Result<Vec<PlanPhase>> {
    let plan_path = project_root.join("PLAN.md");
    if !plan_path.exists() {
        anyhow::bail!("No PLAN.md found in {}", project_root.display());
    }
    let content = std::fs::read_to_string(&plan_path)?;
    Ok(parse_plan(&content))
}

/// Format a plan phase list as a checklist for CLAUDE.md injection.
pub fn format_plan_checklist(phases: &[PlanPhase], current_phase: Option<&str>) -> String {
    let mut lines = Vec::new();
    for phase in phases {
        let checkbox = if phase.status == PlanStatus::Done {
            "[x]"
        } else {
            "[ ]"
        };
        let current_marker = if current_phase == Some(phase.id.as_str()) {
            " <-- current"
        } else {
            ""
        };
        let bold = if current_phase == Some(phase.id.as_str()) {
            format!("**Phase {} — {}**", phase.id, phase.title)
        } else {
            format!("Phase {} — {}", phase.id, phase.title)
        };
        lines.push(format!("- {} {}{}", checkbox, bold, current_marker));
    }
    lines.join("\n")
}

/// Find the next pending phase after the given phase ID.
/// If `after_phase` is None, returns the first pending phase.
pub fn find_next_pending<'a>(
    phases: &'a [PlanPhase],
    after_phase: Option<&str>,
) -> Option<&'a PlanPhase> {
    let start_idx = if let Some(after) = after_phase {
        phases
            .iter()
            .position(|p| p.id == after)
            .map(|i| i + 1)
            .unwrap_or(0)
    } else {
        0
    };

    phases[start_idx..]
        .iter()
        .find(|p| p.status == PlanStatus::Pending)
}

/// Record a plan phase status change to the history log.
pub fn record_history(
    project_root: &Path,
    phase_id: &str,
    old_status: &PlanStatus,
    new_status: &PlanStatus,
) -> anyhow::Result<()> {
    let ta_dir = project_root.join(".ta");
    std::fs::create_dir_all(&ta_dir)?;
    let history_path = ta_dir.join("plan_history.jsonl");

    let entry = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "phase_id": phase_id,
        "old_status": old_status.to_string(),
        "new_status": new_status.to_string(),
    });

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&history_path)?;
    writeln!(file, "{}", entry)?;
    Ok(())
}

/// Load plan history entries from the JSONL file.
pub fn load_history(project_root: &Path) -> anyhow::Result<Vec<serde_json::Value>> {
    let history_path = project_root.join(".ta/plan_history.jsonl");
    if !history_path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&history_path)?;
    let entries: Vec<serde_json::Value> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Ok(entries)
}

/// Build a suggested `ta run` command for the next pending phase.
pub fn suggest_next_goal_command(phase: &PlanPhase) -> String {
    format!(
        "ta run \"implement {}\" --source . --phase {}",
        phase.title, phase.id
    )
}

// ── CLI implementations ──────────────────────────────────────────

fn list_phases(config: &GatewayConfig) -> anyhow::Result<()> {
    let phases = load_plan(&config.workspace_root)?;

    if phases.is_empty() {
        println!("No phases found in PLAN.md.");
        return Ok(());
    }

    println!("{:<12} {:<40} {:<14}", "PHASE", "TITLE", "STATUS");
    println!("{}", "-".repeat(66));

    for phase in &phases {
        let status_display = match phase.status {
            PlanStatus::Done => "done",
            PlanStatus::InProgress => "in_progress",
            PlanStatus::Pending => "pending",
        };
        println!(
            "{:<12} {:<40} {:<14}",
            phase.id,
            truncate(&phase.title, 38),
            status_display,
        );
    }

    Ok(())
}

fn show_status(config: &GatewayConfig) -> anyhow::Result<()> {
    let phases = load_plan(&config.workspace_root)?;

    let done = phases
        .iter()
        .filter(|p| p.status == PlanStatus::Done)
        .count();
    let in_progress = phases
        .iter()
        .filter(|p| p.status == PlanStatus::InProgress)
        .count();
    let pending = phases
        .iter()
        .filter(|p| p.status == PlanStatus::Pending)
        .count();
    let total = phases.len();

    println!("Plan Progress: {}/{} phases complete", done, total);
    println!("  Done:        {}", done);
    println!("  In Progress: {}", in_progress);
    println!("  Pending:     {}", pending);

    if let Some(current) = phases.iter().find(|p| p.status == PlanStatus::InProgress) {
        println!("\nCurrent: Phase {} — {}", current.id, current.title);
    }

    if let Some(next) = phases.iter().find(|p| p.status == PlanStatus::Pending) {
        println!("Next:    Phase {} — {}", next.id, next.title);
    }

    Ok(())
}

fn show_next(config: &GatewayConfig) -> anyhow::Result<()> {
    let phases = load_plan(&config.workspace_root)?;

    // Find next pending (prefer after in_progress, fallback to first pending).
    let after_current = phases
        .iter()
        .rev()
        .find(|p| p.status == PlanStatus::InProgress)
        .map(|p| p.id.as_str());

    let next = find_next_pending(&phases, after_current);

    match next {
        Some(phase) => {
            println!("Next pending phase:");
            println!("  Phase {} — {}", phase.id, phase.title);
            println!();
            println!("To start working on it:");
            println!("  {}", suggest_next_goal_command(phase));
        }
        None => {
            println!("All plan phases are complete or in progress.");
        }
    }

    Ok(())
}

fn show_history(config: &GatewayConfig) -> anyhow::Result<()> {
    let entries = load_history(&config.workspace_root)?;

    if entries.is_empty() {
        println!("No plan history recorded yet.");
        println!("History is recorded when phases change status via `ta draft apply`.");
        return Ok(());
    }

    println!(
        "{:<24} {:<14} {:<14} {:<14}",
        "TIMESTAMP", "PHASE", "FROM", "TO"
    );
    println!("{}", "-".repeat(66));

    for entry in &entries {
        let ts = entry["timestamp"]
            .as_str()
            .unwrap_or("?")
            .chars()
            .take(19)
            .collect::<String>();
        let phase = entry["phase_id"].as_str().unwrap_or("?");
        let from = entry["old_status"].as_str().unwrap_or("?");
        let to = entry["new_status"].as_str().unwrap_or("?");
        println!("{:<24} {:<14} {:<14} {:<14}", ts, phase, from, to);
    }

    Ok(())
}

fn validate_phase(config: &GatewayConfig, phase_id: &str) -> anyhow::Result<()> {
    let phases = load_plan(&config.workspace_root)?;

    let phase = phases.iter().find(|p| p.id == phase_id);
    match phase {
        None => {
            anyhow::bail!("Phase '{}' not found in PLAN.md", phase_id);
        }
        Some(p) => {
            println!("Phase {} — {}", p.id, p.title);
            println!("Status: {}", p.status);

            // Look for the most recent goal linked to this phase.
            let goal_store = ta_goal::GoalRunStore::new(&config.goals_dir)?;
            let goals = goal_store.list()?;
            let phase_goals: Vec<_> = goals
                .iter()
                .filter(|g| g.plan_phase.as_deref() == Some(phase_id))
                .collect();

            if phase_goals.is_empty() {
                println!("\nNo goals found linked to this phase.");
                if p.status != PlanStatus::Done {
                    println!("To start: {}", suggest_next_goal_command(p));
                }
                return Ok(());
            }

            println!("\nLinked goals ({}):", phase_goals.len());
            for g in &phase_goals {
                println!(
                    "  {} — {} [{}]",
                    &g.goal_run_id.to_string()[..8],
                    g.title,
                    g.state,
                );
            }

            // Check if the most recent goal has a draft with change_summary.
            if let Some(latest) = phase_goals.first() {
                if let Some(pkg_id) = latest.pr_package_id {
                    let pkg_path = config.pr_packages_dir.join(format!("{}.json", pkg_id));
                    if pkg_path.exists() {
                        let content = std::fs::read_to_string(&pkg_path)?;
                        if let Ok(pkg) =
                            serde_json::from_str::<ta_changeset::DraftPackage>(&content)
                        {
                            println!("\nLatest draft summary: {}", pkg.summary.what_changed);
                            println!("  Artifacts: {}", pkg.changes.artifacts.len());
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PLAN: &str = r#"# Trusted Autonomy — Development Plan

## Phase 0 — Repo Layout
<!-- status: done -->
Basic repo structure.

## Phase 1 — Kernel
<!-- status: done -->
Core crates.

## Phase 4a — Agent Prompt Enhancement
<!-- status: done -->
CLAUDE.md injection.

## Phase 4a.1 — Plan Tracking
<!-- status: in_progress -->
This very feature.

## Phase 4b — Per-Artifact Review Model
<!-- status: pending -->
Selective approval.

## Phase 4c — Selective Review CLI
<!-- status: pending -->
Wildcards in approve/reject/discuss.
"#;

    const SAMPLE_PLAN_WITH_SUBPHASES: &str = r#"# Plan

## Phase 0 — Repo Layout
<!-- status: done -->

## v0.3 — Review & Plan Automation *(release)*

### v0.3.0 — Review Sessions
<!-- status: done -->
Review sessions.

### v0.3.1 — Plan Lifecycle Automation
<!-- status: pending -->
Automation features.

### v0.3.2 — Configurable Release Pipeline
<!-- status: pending -->
Release automation.
"#;

    #[test]
    fn parse_plan_extracts_all_phases() {
        let phases = parse_plan(SAMPLE_PLAN);
        assert_eq!(phases.len(), 6);
        assert_eq!(phases[0].id, "0");
        assert_eq!(phases[0].title, "Repo Layout");
        assert_eq!(phases[0].status, PlanStatus::Done);
    }

    #[test]
    fn parse_plan_handles_dotted_ids() {
        let phases = parse_plan(SAMPLE_PLAN);
        let phase_4a1 = &phases[3];
        assert_eq!(phase_4a1.id, "4a.1");
        assert_eq!(phase_4a1.title, "Plan Tracking");
        assert_eq!(phase_4a1.status, PlanStatus::InProgress);
    }

    #[test]
    fn parse_plan_handles_all_statuses() {
        let phases = parse_plan(SAMPLE_PLAN);
        let statuses: Vec<&PlanStatus> = phases.iter().map(|p| &p.status).collect();
        assert_eq!(
            statuses,
            vec![
                &PlanStatus::Done,
                &PlanStatus::Done,
                &PlanStatus::Done,
                &PlanStatus::InProgress,
                &PlanStatus::Pending,
                &PlanStatus::Pending,
            ]
        );
    }

    #[test]
    fn update_phase_status_changes_target() {
        let updated = update_phase_status(SAMPLE_PLAN, "4b", PlanStatus::Done);
        let phases = parse_plan(&updated);
        let phase_4b = phases.iter().find(|p| p.id == "4b").unwrap();
        assert_eq!(phase_4b.status, PlanStatus::Done);
    }

    #[test]
    fn update_phase_status_preserves_others() {
        let updated = update_phase_status(SAMPLE_PLAN, "4b", PlanStatus::Done);
        let phases = parse_plan(&updated);
        // Phase 0 still done.
        assert_eq!(phases[0].status, PlanStatus::Done);
        // Phase 4c still pending.
        let phase_4c = phases.iter().find(|p| p.id == "4c").unwrap();
        assert_eq!(phase_4c.status, PlanStatus::Pending);
    }

    #[test]
    fn update_nonexistent_phase_is_noop() {
        let updated = update_phase_status(SAMPLE_PLAN, "99", PlanStatus::Done);
        // Content should be unchanged (no crash, no corruption).
        let phases = parse_plan(&updated);
        assert_eq!(phases.len(), 6);
    }

    #[test]
    fn format_plan_checklist_marks_current() {
        let phases = parse_plan(SAMPLE_PLAN);
        let checklist = format_plan_checklist(&phases, Some("4a.1"));
        assert!(checklist.contains("[x] Phase 0"));
        assert!(checklist.contains("[ ] **Phase 4a.1 — Plan Tracking** <-- current"));
        assert!(checklist.contains("[ ] Phase 4b"));
    }

    #[test]
    fn plan_status_display() {
        assert_eq!(PlanStatus::Done.to_string(), "done");
        assert_eq!(PlanStatus::InProgress.to_string(), "in_progress");
        assert_eq!(PlanStatus::Pending.to_string(), "pending");
    }

    // ── New tests for v0.3.1 features ──

    #[test]
    fn parse_plan_handles_sub_phases() {
        let phases = parse_plan(SAMPLE_PLAN_WITH_SUBPHASES);
        // Should find: Phase 0, v0.3.0, v0.3.1, v0.3.2
        assert_eq!(phases.len(), 4);
        assert_eq!(phases[0].id, "0");
        assert_eq!(phases[1].id, "v0.3.0");
        assert_eq!(phases[1].title, "Review Sessions");
        assert_eq!(phases[1].status, PlanStatus::Done);
        assert_eq!(phases[2].id, "v0.3.1");
        assert_eq!(phases[2].title, "Plan Lifecycle Automation");
        assert_eq!(phases[2].status, PlanStatus::Pending);
    }

    #[test]
    fn update_sub_phase_status() {
        let updated = update_phase_status(SAMPLE_PLAN_WITH_SUBPHASES, "v0.3.1", PlanStatus::Done);
        let phases = parse_plan(&updated);
        let phase = phases.iter().find(|p| p.id == "v0.3.1").unwrap();
        assert_eq!(phase.status, PlanStatus::Done);
        // v0.3.0 should still be done.
        let v030 = phases.iter().find(|p| p.id == "v0.3.0").unwrap();
        assert_eq!(v030.status, PlanStatus::Done);
        // v0.3.2 should still be pending.
        let v032 = phases.iter().find(|p| p.id == "v0.3.2").unwrap();
        assert_eq!(v032.status, PlanStatus::Pending);
    }

    #[test]
    fn find_next_pending_returns_first() {
        let phases = parse_plan(SAMPLE_PLAN);
        let next = find_next_pending(&phases, None);
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, "4b");
    }

    #[test]
    fn find_next_pending_after_phase() {
        let phases = parse_plan(SAMPLE_PLAN);
        let next = find_next_pending(&phases, Some("4b"));
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, "4c");
    }

    #[test]
    fn find_next_pending_returns_none_when_all_done() {
        let plan = r#"
## Phase 0 — Done
<!-- status: done -->
"#;
        let phases = parse_plan(plan);
        let next = find_next_pending(&phases, None);
        assert!(next.is_none());
    }

    #[test]
    fn find_next_pending_sub_phases() {
        let phases = parse_plan(SAMPLE_PLAN_WITH_SUBPHASES);
        let next = find_next_pending(&phases, Some("v0.3.0"));
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, "v0.3.1");
    }

    #[test]
    fn suggest_next_goal_command_format() {
        let phase = PlanPhase {
            id: "v0.3.2".to_string(),
            title: "Release Pipeline".to_string(),
            status: PlanStatus::Pending,
        };
        let cmd = suggest_next_goal_command(&phase);
        assert_eq!(
            cmd,
            "ta run \"implement Release Pipeline\" --source . --phase v0.3.2"
        );
    }

    #[test]
    fn record_and_load_history() {
        let dir = tempfile::tempdir().unwrap();
        record_history(
            dir.path(),
            "v0.3.1",
            &PlanStatus::Pending,
            &PlanStatus::Done,
        )
        .unwrap();
        record_history(
            dir.path(),
            "v0.3.2",
            &PlanStatus::Pending,
            &PlanStatus::InProgress,
        )
        .unwrap();

        let entries = load_history(dir.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["phase_id"], "v0.3.1");
        assert_eq!(entries[0]["new_status"], "done");
        assert_eq!(entries[1]["phase_id"], "v0.3.2");
        assert_eq!(entries[1]["new_status"], "in_progress");
    }

    #[test]
    fn load_history_empty_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let entries = load_history(dir.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn extract_phase_id_from_top_level_header() {
        assert_eq!(
            extract_phase_id_from_header("## Phase 4b — Per-Artifact Review Model"),
            Some("4b".to_string())
        );
    }

    #[test]
    fn extract_phase_id_from_sub_header() {
        assert_eq!(
            extract_phase_id_from_header("### v0.3.1 — Plan Lifecycle Automation"),
            Some("v0.3.1".to_string())
        );
    }

    #[test]
    fn extract_phase_id_from_unrelated_header() {
        assert_eq!(
            extract_phase_id_from_header("## Versioning & Release Policy"),
            None
        );
    }
}
