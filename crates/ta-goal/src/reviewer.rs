// reviewer.rs — Per-item completion verification for plan phases (v0.15.24.1).
//
// When `ta draft apply` runs for a phase, the reviewer verifies each plan item:
// - For `[x]` items: confirms implementing code exists in the diff.
// - For `[ ]` items: checks if code was written despite the item being unchecked.
//   If code is found, auto-corrects the PLAN.md checkbox and notes the discrepancy.
//
// The result is a per-item completion table surfaced in `ta draft view`.

use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────────

/// Completion status for one plan item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemCompletionStatus {
    /// 1-based item number within the phase.
    pub item_number: usize,
    /// The item text (without checkbox prefix).
    pub text: String,
    /// Whether the agent marked this item `[x]`.
    pub marked: bool,
    /// Whether implementing code was found in the diff content.
    pub code_verified: bool,
    /// If true, the reviewer found code for an unchecked item and auto-corrected it.
    pub auto_corrected: bool,
    /// Optional note (e.g. "[auto-corrected] item N was implemented but not marked").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ItemCompletionStatus {
    /// One-line status string for display.
    pub fn status_line(&self) -> String {
        let check = if self.marked { "[x]" } else { "[ ]" };
        let verified = if self.code_verified { "✓" } else { "?" };
        let ac = if self.auto_corrected {
            " [auto-corrected]"
        } else {
            ""
        };
        let excerpt: String = self.text.chars().take(60).collect();
        format!(
            "  {} {} code:{}{} {}",
            self.item_number, check, verified, ac, excerpt
        )
    }
}

/// Result of per-item completion verification for a plan phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionReport {
    /// Phase ID this report covers.
    pub phase_id: String,
    /// Per-item completion status table.
    pub items: Vec<ItemCompletionStatus>,
    /// Number of items that were auto-corrected from `[ ]` to `[x]`.
    pub auto_corrected_count: usize,
    /// Number of items marked `[x]` by the agent.
    pub marked_count: usize,
    /// Number of items with code coverage verified.
    pub verified_count: usize,
    /// Total items in the phase.
    pub total_items: usize,
}

impl CompletionReport {
    /// One-line summary for `ta draft view`.
    pub fn summary_line(&self) -> String {
        format!(
            "phase {}: {}/{} marked, {}/{} code-verified, {} auto-corrected",
            self.phase_id,
            self.marked_count,
            self.total_items,
            self.verified_count,
            self.total_items,
            self.auto_corrected_count,
        )
    }

    /// Returns true if all items are either marked or code-verified.
    pub fn is_complete(&self) -> bool {
        self.items
            .iter()
            .all(|i| i.marked || i.code_verified || i.auto_corrected)
    }
}

// ── Core logic ────────────────────────────────────────────────────────────────

/// Extract significant tokens from a plan item for code coverage checking.
/// Filters stop-words and short tokens. Returns up to 4 tokens.
fn extract_tokens(text: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "an", "the", "and", "or", "in", "on", "at", "to", "for", "of", "with", "by", "from",
        "as", "is", "it", "its", "be", "this", "that", "are", "was", "will", "should", "when",
        "if", "all", "any", "not", "no", "so", "do", "new", "each", "per", "via", "into", "add",
        "use", "run", "get", "set", "has", "can", "may", "must", "after", "before", "up", "out",
        "then", "than",
    ];

    let tokens: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| {
            let lower = t.to_lowercase();
            t.len() >= 4 && !STOP_WORDS.contains(&lower.as_str())
        })
        .map(|t| t.to_lowercase())
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut unique: Vec<String> = Vec::new();
    for token in tokens {
        if seen.insert(token.clone()) {
            unique.push(token);
            if unique.len() >= 4 {
                break;
            }
        }
    }
    unique
}

/// Check whether an item's tokens appear in the diff/code content.
fn has_code_coverage(item_text: &str, diff_content: &str) -> bool {
    let tokens = extract_tokens(item_text);
    if tokens.is_empty() {
        return false;
    }
    let diff_lower = diff_content.to_lowercase();
    tokens.iter().any(|tok| diff_lower.contains(tok.as_str()))
}

/// Parse `[ ]` and `[x]` items from a specific phase in PLAN.md.
///
/// Returns `Vec<(item_number, text, checked)>`.
pub fn parse_phase_items(plan_content: &str, phase_id: &str) -> Vec<(usize, String, bool)> {
    let lines: Vec<&str> = plan_content.lines().collect();

    // Find the phase header.
    let phase_start = find_phase_start(&lines, phase_id);
    let phase_start = match phase_start {
        Some(idx) => idx,
        None => return vec![],
    };

    let mut items = Vec::new();
    let mut item_number = 0usize;
    let mut i = phase_start + 1;

    while i < lines.len() {
        let line = lines[i].trim();

        // Stop at the next phase (### or deeper section at same level).
        if (line.starts_with("## ") || line.starts_with("### ")) && i > phase_start {
            break;
        }

        // Match both bullet list (`- [x]`) and numbered list (`1. [x]`, `10. [x]`) styles.
        let checkbox_content = extract_checkbox_content(line);
        if let Some((checked, rest)) = checkbox_content {
            item_number += 1;
            items.push((item_number, rest.trim().to_string(), checked));
        }
        i += 1;
    }

    items
}

/// Verify per-item completion for a plan phase.
///
/// - `plan_content`: content of PLAN.md (staging version, with agent's checkboxes)
/// - `diff_content`: concatenated code content from all changed artifacts
/// - `phase_id`: e.g. "v0.15.24.1"
pub fn verify_phase_completion(
    plan_content: &str,
    diff_content: &str,
    phase_id: &str,
) -> CompletionReport {
    let raw_items = parse_phase_items(plan_content, phase_id);
    let total_items = raw_items.len();
    let mut items = Vec::with_capacity(total_items);
    let mut auto_corrected_count = 0usize;
    let mut marked_count = 0usize;
    let mut verified_count = 0usize;

    for (item_number, text, marked) in &raw_items {
        let code_found = has_code_coverage(text, diff_content);
        let auto_corrected = !marked && code_found;
        let note = if auto_corrected {
            Some(format!(
                "[auto-corrected] item {} was implemented but not marked complete by agent",
                item_number
            ))
        } else {
            None
        };

        if *marked {
            marked_count += 1;
        }
        if code_found {
            verified_count += 1;
        }
        if auto_corrected {
            auto_corrected_count += 1;
        }

        items.push(ItemCompletionStatus {
            item_number: *item_number,
            text: text.clone(),
            marked: *marked || auto_corrected,
            code_verified: code_found,
            auto_corrected,
            note,
        });
    }

    CompletionReport {
        phase_id: phase_id.to_string(),
        items,
        auto_corrected_count,
        marked_count,
        verified_count,
        total_items,
    }
}

/// Apply auto-corrections from a `CompletionReport` to PLAN.md content.
///
/// For each item marked `auto_corrected`, replaces `[ ] <text>` with `[x] <text>`
/// in the plan content (handles both `- [ ]` and `N. [ ]` prefixes).
/// Returns the updated content.
pub fn auto_correct_plan_md(plan_content: &str, report: &CompletionReport) -> String {
    let mut result = plan_content.to_string();

    for item in &report.items {
        if item.auto_corrected {
            // Replace the first occurrence of `[ ] <text>` with `[x] <text>`.
            // This works for both `- [ ] text` and `1. [ ] text`.
            let pattern = format!("[ ] {}", item.text);
            let replacement = format!("[x] {}", item.text);
            if result.contains(&pattern) {
                result = result.replacen(&pattern, &replacement, 1);
                tracing::info!(
                    item = item.item_number,
                    text = %item.text.chars().take(60).collect::<String>(),
                    "reviewer auto-corrected unchecked plan item"
                );
            }
        }
    }

    result
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Extract checkbox state and text from a line.
///
/// Handles both:
/// - Bullet style: `- [x] text` or `- [ ] text`
/// - Numbered style: `1. [x] text` or `42. [ ] text`
///
/// Returns `Some((checked, rest_of_line))` or `None` if no checkbox found.
fn extract_checkbox_content(line: &str) -> Option<(bool, &str)> {
    // Strip leading `- ` or `N. ` prefix.
    let after_prefix = if let Some(rest) = line.strip_prefix("- ") {
        rest
    } else {
        // Try `<digits>. ` prefix.
        let dot_pos = line.find(". ")?;
        if line[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            &line[dot_pos + 2..]
        } else {
            return None;
        }
    };

    if let Some(rest) = after_prefix
        .strip_prefix("[x] ")
        .or_else(|| after_prefix.strip_prefix("[X] "))
    {
        Some((true, rest))
    } else if let Some(rest) = after_prefix.strip_prefix("[ ] ") {
        Some((false, rest))
    } else if after_prefix == "[x]" || after_prefix == "[X]" {
        Some((true, ""))
    } else if after_prefix == "[ ]" {
        Some((false, ""))
    } else {
        None
    }
}

fn find_phase_start(lines: &[&str], phase_id: &str) -> Option<usize> {
    let id_norm = phase_id.strip_prefix('v').unwrap_or(phase_id);
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("### ") {
            continue;
        }
        let rest = &trimmed[4..];
        let has_id = rest.starts_with(phase_id)
            || rest.starts_with(&format!("v{}", id_norm))
            || rest.starts_with(id_norm);
        if has_id {
            return Some(idx);
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN_WITH_MIXED_ITEMS: &str = r#"
### v0.15.24.1 — Audit Trail Integrity
<!-- status: pending -->

#### Items

1. [x] Bundle audit files into draft apply commit (`apps/ta-cli/src/commands/draft.rs`)
2. [ ] Ordering validation (`crates/ta-goal/src/audit.rs`)
3. [x] Implementation agent task-marking requirement

"#;

    #[test]
    fn parse_phase_items_extracts_checked_and_unchecked() {
        let items = parse_phase_items(PLAN_WITH_MIXED_ITEMS, "v0.15.24.1");
        assert_eq!(items.len(), 3);
        assert!(items[0].2, "item 1 should be checked");
        assert!(!items[1].2, "item 2 should be unchecked");
        assert!(items[2].2, "item 3 should be checked");
    }

    #[test]
    fn parse_phase_items_stops_at_next_phase() {
        let plan = r#"
### v0.15.1 — Phase A
<!-- status: pending -->
- [x] Do thing A
- [ ] Do thing B

### v0.15.2 — Phase B
<!-- status: pending -->
- [x] Do thing C
"#;
        let items = parse_phase_items(plan, "v0.15.1");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].1, "Do thing A");
        assert_eq!(items[1].1, "Do thing B");
    }

    #[test]
    fn verify_phase_completion_marks_code_verified() {
        // diff_content contains tokens from item 2 ("ordering validation audit")
        let diff = "fn validate_ordering audit trail jsonl";
        let report = verify_phase_completion(PLAN_WITH_MIXED_ITEMS, diff, "v0.15.24.1");

        assert_eq!(report.total_items, 3);
        // item 2 was unchecked but has code coverage → auto_corrected
        let item2 = report.items.iter().find(|i| i.item_number == 2).unwrap();
        assert!(item2.code_verified, "item 2 tokens found in diff");
        assert!(item2.auto_corrected, "item 2 should be auto-corrected");
        assert!(item2.marked, "auto-corrected item should be marked true");
        assert_eq!(report.auto_corrected_count, 1);
    }

    #[test]
    fn verify_phase_completion_no_coverage_stays_unchecked() {
        let diff = "completely unrelated code with no matching tokens";
        let report = verify_phase_completion(PLAN_WITH_MIXED_ITEMS, diff, "v0.15.24.1");

        let item2 = report.items.iter().find(|i| i.item_number == 2).unwrap();
        assert!(!item2.code_verified);
        assert!(!item2.auto_corrected);
        assert!(!item2.marked);
    }

    #[test]
    fn auto_correct_plan_md_updates_checkbox() {
        let diff = "fn validate_ordering audit trail implementation";
        let report = verify_phase_completion(PLAN_WITH_MIXED_ITEMS, diff, "v0.15.24.1");

        let corrected = auto_correct_plan_md(PLAN_WITH_MIXED_ITEMS, &report);
        // item 2 should now be [x] in the output
        assert!(
            corrected.contains("[x] Ordering validation"),
            "auto-corrected item should have [x] checkbox"
        );
        assert!(
            !corrected.contains("[ ] Ordering validation"),
            "original [ ] should be replaced"
        );
    }

    #[test]
    fn completion_report_summary_line() {
        let diff = "validate_ordering audit";
        let report = verify_phase_completion(PLAN_WITH_MIXED_ITEMS, diff, "v0.15.24.1");
        let summary = report.summary_line();
        assert!(summary.contains("v0.15.24.1"));
        assert!(summary.contains("auto-corrected"));
    }

    #[test]
    fn reviewer_summary_contains_per_item_table() {
        let diff = "";
        let report = verify_phase_completion(PLAN_WITH_MIXED_ITEMS, diff, "v0.15.24.1");
        let table: Vec<String> = report.items.iter().map(|i| i.status_line()).collect();
        assert_eq!(table.len(), 3);
        for line in &table {
            // Each line must include the item number and a checkbox indicator.
            assert!(
                line.contains("[x]") || line.contains("[ ]"),
                "each row must show checkbox state: {}",
                line
            );
        }
    }

    #[test]
    fn unknown_phase_returns_empty_report() {
        let report = verify_phase_completion(PLAN_WITH_MIXED_ITEMS, "", "v9.9.9");
        assert_eq!(report.total_items, 0);
        assert!(report.items.is_empty());
    }
}
