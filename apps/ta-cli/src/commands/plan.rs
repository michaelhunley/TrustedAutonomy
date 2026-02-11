// plan.rs — Plan tracking: parse PLAN.md, show status, update phases.
//
// The canonical project plan lives in PLAN.md at the project root.
// Each phase has a machine-parseable status marker:
//   ## Phase 4b — Per-Artifact Review Model
//   <!-- status: pending -->
//
// `ta plan list` shows all phases with their status.
// `ta plan status` shows a summary of progress.
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
}

pub fn execute(cmd: &PlanCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match cmd {
        PlanCommands::List => list_phases(config),
        PlanCommands::Status => show_status(config),
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
/// Expects headers like:
///   ## Phase 4b — Per-Artifact Review Model
///   <!-- status: pending -->
pub fn parse_plan(content: &str) -> Vec<PlanPhase> {
    let mut phases = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        // Match phase headers: ## Phase <id> — <title>
        // Support both em dash (—) and regular dash (-).
        if let Some(rest) = line
            .strip_prefix("## Phase ")
            .or_else(|| line.strip_prefix("## Phase\u{a0}"))
        {
            // Split on em dash or space-dash-space.
            let (id, title) = if let Some(pos) = rest.find(" — ") {
                (&rest[..pos], rest[pos + " — ".len()..].trim())
            } else if let Some(pos) = rest.find(" - ") {
                (&rest[..pos], rest[pos + " - ".len()..].trim())
            } else {
                (rest.trim(), "")
            };

            let id = id.trim().to_string();
            let title = title.to_string();

            // Look for status marker on the next line.
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
/// Finds the phase by ID and replaces its status marker.
pub fn update_phase_status(content: &str, phase_id: &str, new_status: PlanStatus) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::with_capacity(lines.len());

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Check if this is the target phase header.
        let is_target = if let Some(rest) = trimmed
            .strip_prefix("## Phase ")
            .or_else(|| trimmed.strip_prefix("## Phase\u{a0}"))
        {
            let parsed_id = if let Some(pos) = rest.find(" — ") {
                rest[..pos].trim()
            } else if let Some(pos) = rest.find(" - ") {
                rest[..pos].trim()
            } else {
                rest.trim()
            };
            parsed_id == phase_id
        } else {
            false
        };

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
}
