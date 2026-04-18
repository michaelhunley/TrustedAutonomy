//! ReviewReport: the output of the draft pre-apply plan review.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::plan_merge::PlanConflict;

/// A plan item that appears unchecked in the phase being applied, with no matching
/// token found in the draft diffs. Informational only — does not block apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageGap {
    pub phase_id: String,
    pub item_number: usize,
    pub text_excerpt: String,
}

/// The result of the automatic PLAN.md audit run during `ta draft build`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewReport {
    pub draft_id: Uuid,
    pub generated_at: DateTime<Utc>,
    /// Regressions silently fixed: source had a newer status and staging reverted it.
    pub silent_fixes: Vec<String>,
    /// Agent-driven additions: newly checked items, phase completions, new sub-phases.
    pub agent_additions: Vec<String>,
    /// Unresolvable conflicts requiring human decision.
    pub conflicts: Vec<PlanConflict>,
    /// Plan items with no token coverage in the diff (informational).
    pub coverage_gaps: Vec<CoverageGap>,
    /// Unified diff against source PLAN.md incorporating all non-conflict resolutions.
    /// `None` when there is nothing to patch (already clean or no PLAN.md in draft).
    pub plan_patch: Option<String>,
}

impl ReviewReport {
    pub fn is_clean(&self) -> bool {
        self.silent_fixes.is_empty()
            && self.agent_additions.is_empty()
            && self.conflicts.is_empty()
            && self.coverage_gaps.is_empty()
    }

    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    pub fn summary_line(&self) -> String {
        format!(
            "{} fixes, {} additions, {} conflicts, {} gaps",
            self.silent_fixes.len(),
            self.agent_additions.len(),
            self.conflicts.len(),
            self.coverage_gaps.len()
        )
    }

    /// Load a ReviewReport from `.ta/review/<draft_id>/report.json`.
    pub fn load(workspace_root: &std::path::Path, draft_id: Uuid) -> Option<Self> {
        let path = workspace_root
            .join(".ta")
            .join("review")
            .join(draft_id.to_string())
            .join("report.json");
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save the ReviewReport to `.ta/review/<draft_id>/report.json`.
    pub fn save(&self, workspace_root: &std::path::Path) -> anyhow::Result<()> {
        let dir = workspace_root
            .join(".ta")
            .join("review")
            .join(self.draft_id.to_string());
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("report.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

/// Render a ReviewReport for display in `ta draft view`.
pub fn render_review_report(report: &ReviewReport, color: bool) -> String {
    if report.is_clean() {
        if color {
            return "\x1b[32m[review] Plan audit clean.\x1b[0m\n".to_string();
        } else {
            return "[review] Plan audit clean.\n".to_string();
        }
    }

    let mut out = String::new();
    out.push_str("[review] Plan audit:\n");

    for fix in &report.silent_fixes {
        if color {
            out.push_str(&format!("  \x1b[90m[fix] {}\x1b[0m\n", fix));
        } else {
            out.push_str(&format!("  [fix] {}\n", fix));
        }
    }

    for addition in &report.agent_additions {
        if color {
            out.push_str(&format!("  \x1b[32m[+] {}\x1b[0m\n", addition));
        } else {
            out.push_str(&format!("  [+] {}\n", addition));
        }
    }

    for gap in &report.coverage_gaps {
        if color {
            out.push_str(&format!(
                "  \x1b[33m[gap] {} item {}: {}\x1b[0m\n",
                gap.phase_id, gap.item_number, gap.text_excerpt
            ));
        } else {
            out.push_str(&format!(
                "  [gap] {} item {}: {}\n",
                gap.phase_id, gap.item_number, gap.text_excerpt
            ));
        }
    }

    for conflict in &report.conflicts {
        if color {
            out.push_str(&format!(
                "  \x1b[31m[CONFLICT] {}\x1b[0m\n",
                conflict.description
            ));
            if !conflict.base_text.is_empty() {
                out.push_str(&format!("    base:    {}\n", conflict.base_text));
            }
            out.push_str(&format!("    staging: {}\n", conflict.staging_text));
            out.push_str(&format!("    source:  {}\n", conflict.source_text));
            out.push_str("    \x1b[31m→ Resolve before applying.\x1b[0m\n");
        } else {
            out.push_str(&format!("  [CONFLICT] {}\n", conflict.description));
            if !conflict.base_text.is_empty() {
                out.push_str(&format!("    base:    {}\n", conflict.base_text));
            }
            out.push_str(&format!("    staging: {}\n", conflict.staging_text));
            out.push_str(&format!("    source:  {}\n", conflict.source_text));
            out.push_str("    → Resolve before applying.\n");
        }
    }

    out
}
