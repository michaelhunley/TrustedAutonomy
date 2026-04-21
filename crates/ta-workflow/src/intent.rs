// intent.rs — Natural language → workflow invocation resolver (v0.15.24).
//
// Resolves natural language phrases to parameterized workflow templates using
// regex-based entity extraction and keyword scoring. No LLM required.
//
// Entry point: `resolve_intent(text, templates, plan_ctx) -> IntentResolution`
//
// Scoring weights:
//   verb match         0-0.25  — intent verb matches template tags
//   scope match        0-0.40  — scope modifier aligns with template structure
//   version ref match  0-0.25  — version ref maps to a phase-aware template
//   phase mention      0-0.25  — explicit "phase" keyword + phase template
//   keyword overlap    0-0.10  — description/tag keyword overlap
//
// Threshold: score ≥ 0.80 → present confirmation card. Below → ask question.

use crate::params::{PlanContext, TemplateEntry};

/// Entities extracted from natural language input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedIntent {
    /// Version reference extracted from text (e.g., "v0.15", "v0.15.24").
    pub version_ref: Option<String>,
    /// Normalized intent verb (implement, build, run, complete, deploy, etc.).
    pub intent_verb: String,
    /// Scope modifier (remaining, pending, all, next, current, or empty).
    pub scope_modifier: String,
    /// Whether the text explicitly contains "phase" or "phases".
    pub mentions_phase: bool,
}

/// A candidate template with its relevance score and derived invocation params.
#[derive(Debug, Clone)]
pub struct TemplateCandidate {
    /// Template name to run.
    pub name: String,
    /// Confidence score (0.0–1.0).
    pub score: f32,
    /// Derived `--param key=value` pairs to pass at invocation time.
    pub params: Vec<String>,
    /// Synthesized goal title for templates that require `goal_title`.
    pub goal_title: Option<String>,
}

/// Outcome of intent resolution.
#[derive(Debug, Clone)]
pub enum IntentResolution {
    /// A template matched at confidence ≥ 0.80. Present the confirmation card.
    Resolved {
        candidate: TemplateCandidate,
        /// Human-readable confirmation card with numbered action options.
        confirmation_card: String,
    },
    /// No template matched at ≥ 0.80. Ask this question before retrying.
    NeedsQuestion(String),
}

/// Extract entities from a natural language phrase.
///
/// Uses regex for version refs and keyword lists for verbs and scope modifiers.
/// No ML required — all matching is deterministic and O(n·k) where k is fixed.
pub fn extract_intent(text: &str) -> ExtractedIntent {
    let lower = text.to_lowercase();

    // Version ref: v<major>.<minor> or v<major>.<minor>.<patch>[.<sub>]
    let version_re = regex::Regex::new(r"\bv(\d+\.\d+(?:\.\d+)*)").expect("static regex");
    let version_ref = version_re
        .captures(&lower)
        .and_then(|c| c.get(0))
        .map(|m| m.as_str().to_string());

    // Intent verb: first match wins (ordered by specificity).
    let verb_table: &[(&str, &[&str])] = &[
        ("implement", &["implement", "implementing"]),
        ("build", &["build", "building"]),
        (
            "complete",
            &["complete", "completing", "finish", "finishing"],
        ),
        ("deploy", &["deploy", "deploying", "release", "releasing"]),
        ("run", &["run", "running", "execute", "executing"]),
        ("do", &["do ", "doing"]),
    ];
    let mut intent_verb = String::new();
    'verb: for (canonical, variants) in verb_table {
        for v in *variants {
            if lower.contains(v) {
                intent_verb = canonical.to_string();
                break 'verb;
            }
        }
    }

    // Scope modifier: first match wins.
    let scope_table: &[(&str, &[&str])] = &[
        ("remaining", &["remaining"]),
        ("pending", &["pending"]),
        ("all", &["all "]),
        ("next", &["next"]),
        ("current", &["current", " this "]),
    ];
    let mut scope_modifier = String::new();
    'scope: for (canonical, variants) in scope_table {
        for v in *variants {
            if lower.contains(v) {
                scope_modifier = canonical.to_string();
                break 'scope;
            }
        }
    }

    let mentions_phase = lower.contains("phase");

    ExtractedIntent {
        version_ref,
        intent_verb,
        scope_modifier,
        mentions_phase,
    }
}

/// Score a single template entry against extracted intent (0.0–1.0).
///
/// Higher score means the template is a better match for the intent.
/// Breakdown:
/// - Verb score  (0–0.25): intent verb present in template tags/description.
/// - Scope score (0–0.40): scope modifier aligns with template's param structure.
/// - Version score (0–0.25): version_ref maps naturally to phase-filter templates.
/// - Phase mention (0–0.25): explicit "phase" text + template has phase_filter.
/// - Keyword overlap (0–0.10): description/tag overlap with extracted entities.
pub fn score_template(entry: &TemplateEntry, intent: &ExtractedIntent) -> f32 {
    let mut score = 0.0f32;

    let has_phase_filter = entry.params.iter().any(|(k, _)| k == "phase_filter");
    let has_goal_title = entry.params.iter().any(|(k, _)| k == "goal_title");

    let tags_lower: Vec<String> = entry.tags.iter().map(|t| t.to_lowercase()).collect();
    let desc_lower = entry.description.to_lowercase();

    // ── 1. Verb match (0–0.25) ──────────────────────────────────────────────
    if !intent.intent_verb.is_empty() {
        let synonyms = verb_synonyms(&intent.intent_verb);
        let in_tags = tags_lower.iter().any(|t| synonyms.iter().any(|s| t == *s));
        let in_desc = synonyms.iter().any(|s| desc_lower.contains(s));
        if in_tags {
            score += 0.25;
        } else if in_desc {
            score += 0.15;
        } else if ["implement", "build", "run", "execute", "complete"]
            .contains(&intent.intent_verb.as_str())
        {
            score += 0.10;
        }
    }

    // ── 2. Scope match (0–0.40) ─────────────────────────────────────────────
    match intent.scope_modifier.as_str() {
        "remaining" | "pending" | "all" => {
            if has_phase_filter {
                score += 0.40;
            } else {
                score += 0.05;
            }
        }
        "next" | "current" => {
            if has_phase_filter && intent.mentions_phase {
                score += 0.35;
            } else if has_phase_filter {
                score += 0.20;
            } else if has_goal_title {
                score += 0.25;
            }
        }
        _ => {}
    }

    // ── 3. Version ref match (0–0.25) ───────────────────────────────────────
    if intent.version_ref.is_some() {
        if has_phase_filter {
            score += 0.25;
        } else {
            score += 0.05;
        }
    }

    // ── 4. Explicit phase mention (0–0.25) ──────────────────────────────────
    if intent.mentions_phase && has_phase_filter {
        score += 0.25;
    }

    // ── 5. Description / tag keyword overlap (0–0.10) ───────────────────────
    let phase_words = ["phase", "phases", "pending", "plan", "iterate", "loop"];
    let desc_has_phase_word = phase_words
        .iter()
        .any(|w| desc_lower.contains(w) || tags_lower.iter().any(|t| t.contains(w)));

    if desc_has_phase_word
        && (!intent.scope_modifier.is_empty()
            || intent.version_ref.is_some()
            || intent.mentions_phase)
    {
        score += 0.10;
    }

    score.min(1.0)
}

/// Resolve natural language to a workflow template invocation.
///
/// Scores all available templates, selects the top candidate, and returns
/// either a confirmation card (score ≥ 0.80) or a clarifying question.
pub fn resolve_intent(
    text: &str,
    templates: &[TemplateEntry],
    plan_ctx: &PlanContext,
) -> IntentResolution {
    let intent = extract_intent(text);

    // Score all templates.
    let mut scored: Vec<(f32, &TemplateEntry)> = templates
        .iter()
        .map(|t| (score_template(t, &intent), t))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let top_score = scored.first().map(|(s, _)| *s).unwrap_or(0.0);

    if top_score < 0.80 {
        return IntentResolution::NeedsQuestion(build_clarifying_question(&intent, templates));
    }

    let (score, entry) = scored[0];
    let params = derive_params(entry, &intent, plan_ctx);
    let goal_title = synthesize_goal_title(entry, &intent, plan_ctx);
    let confirmation_card = build_confirmation_card(entry, &params, goal_title.as_deref(), score);

    IntentResolution::Resolved {
        candidate: TemplateCandidate {
            name: entry.name.clone(),
            score,
            params,
            goal_title,
        },
        confirmation_card,
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// Map a canonical intent verb to its synonyms (for tag/description matching).
fn verb_synonyms(verb: &str) -> Vec<&'static str> {
    match verb {
        "implement" => vec!["implement", "build", "code"],
        "build" => vec!["build", "implement", "compile"],
        "run" => vec!["run", "execute", "start"],
        "complete" => vec!["complete", "finish", "done"],
        "deploy" => vec!["deploy", "release", "publish"],
        "do" => vec!["do", "run", "execute"],
        _ => vec![],
    }
}

/// Derive `--param key=value` pairs from the intent + plan context.
fn derive_params(
    entry: &TemplateEntry,
    intent: &ExtractedIntent,
    plan_ctx: &PlanContext,
) -> Vec<String> {
    let mut params = Vec::new();

    let has_phase_filter = entry.params.iter().any(|(k, _)| k == "phase_filter");

    if has_phase_filter {
        let filter_value = if let Some(ref vref) = intent.version_ref {
            vref.clone()
        } else if intent.scope_modifier == "next" || intent.scope_modifier == "current" {
            // Narrow to the specific next pending phase prefix.
            if !plan_ctx.next_pending_phase.is_empty() {
                crate::params::version_prefix_from_id(&plan_ctx.next_pending_phase)
            } else {
                plan_ctx.current_version_prefix.clone()
            }
        } else {
            // remaining/pending/all or no scope — use current version prefix as default.
            plan_ctx.current_version_prefix.clone()
        };

        if !filter_value.is_empty() {
            params.push(format!("phase_filter={}", filter_value));
        }
    }

    params
}

/// Synthesize a goal title for the invocation.
fn synthesize_goal_title(
    entry: &TemplateEntry,
    intent: &ExtractedIntent,
    plan_ctx: &PlanContext,
) -> Option<String> {
    let has_goal_title = entry.params.iter().any(|(k, _)| k == "goal_title");
    if !has_goal_title && entry.params.iter().any(|(k, _)| k == "phase_filter") {
        // plan-build-phases synthesizes its own goal from phase_filter.
        let scope = match intent.scope_modifier.as_str() {
            "remaining" | "pending" => "remaining",
            "all" => "all",
            "next" | "current" => "next",
            _ => "pending",
        };
        let version_label = intent
            .version_ref
            .as_deref()
            .filter(|v| !v.is_empty())
            .unwrap_or(plan_ctx.current_version_prefix.as_str());
        Some(format!("Build {} {} phases", scope, version_label))
    } else {
        None
    }
}

/// Build the numbered confirmation card shown when score ≥ 0.80.
fn build_confirmation_card(
    entry: &TemplateEntry,
    params: &[String],
    goal_title: Option<&str>,
    score: f32,
) -> String {
    let param_flags: String = params
        .iter()
        .map(|p| format!(" --param {}", p))
        .collect::<Vec<_>>()
        .join("");

    let cmd = format!("ta workflow run {}{}", entry.name, param_flags);

    let mut card = String::new();
    card.push_str(&format!(
        "Resolved workflow  [{:.0}% confidence]\n",
        score * 100.0
    ));
    card.push_str(&format!("  Template : {}\n", entry.name));
    card.push_str(&format!("  Command  : {}\n", cmd));
    if let Some(title) = goal_title {
        card.push_str(&format!("  Goal     : {}\n", title));
    }
    if !entry.description.is_empty() {
        card.push_str(&format!("  About    : {}\n", entry.description));
    }
    card.push('\n');
    card.push_str("1. Run   2. Adjust   3. Different workflow   4. Cancel");
    card
}

/// Build a clarifying question for low-confidence cases.
fn build_clarifying_question(intent: &ExtractedIntent, templates: &[TemplateEntry]) -> String {
    let mut lines = Vec::new();
    lines.push("I couldn't confidently match that to a workflow template.".to_string());
    lines.push(String::new());

    if !templates.is_empty() {
        lines.push("Available templates:".to_string());
        for t in templates.iter().take(5) {
            lines.push(format!("  {}  — {}", t.name, t.description));
        }
        lines.push(String::new());
    }

    let mut hints = Vec::new();
    if intent.version_ref.is_none() {
        hints.push("a version (e.g., \"v0.15\")");
    }
    if intent.scope_modifier.is_empty() {
        hints.push("a scope (remaining / next / all)");
    }
    if intent.intent_verb.is_empty() {
        hints.push("an action verb (implement / build / run)");
    }

    if hints.is_empty() {
        lines.push(
            "Try specifying the template name explicitly: ta workflow run <name>".to_string(),
        );
    } else {
        lines.push(format!(
            "Try adding {} to your request, or use the template name directly:",
            hints.join(", ")
        ));
        lines.push("  ta workflow run plan-build-phases --param phase_filter=v0.15".to_string());
        lines.push("  ta workflow run governed-goal --goal \"Fix the auth bug\"".to_string());
    }

    lines.join("\n")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{TemplateEntry, TemplateSource};

    fn phase_template() -> TemplateEntry {
        TemplateEntry {
            name: "plan-build-phases".to_string(),
            description: "Iterate pending PLAN.md phases through the governed build workflow."
                .to_string(),
            source: TemplateSource::Builtin,
            params: vec![
                (
                    "phase_filter".to_string(),
                    "string — Phase prefix to filter".to_string(),
                ),
                (
                    "max_phases".to_string(),
                    "integer — Maximum phases to run".to_string(),
                ),
            ],
            tags: vec![
                "plan".to_string(),
                "phases".to_string(),
                "build".to_string(),
                "implement".to_string(),
                "run".to_string(),
                "pending".to_string(),
                "remaining".to_string(),
                "iterate".to_string(),
            ],
        }
    }

    fn goal_template() -> TemplateEntry {
        TemplateEntry {
            name: "governed-goal".to_string(),
            description:
                "Safe autonomous coding loop: run_goal → review → human_gate → apply → pr_sync."
                    .to_string(),
            source: TemplateSource::Builtin,
            params: vec![
                (
                    "goal_title".to_string(),
                    "string [required] — Goal title".to_string(),
                ),
                ("phase".to_string(), "string — Phase ID".to_string()),
            ],
            tags: vec![
                "goal".to_string(),
                "implement".to_string(),
                "build".to_string(),
                "run".to_string(),
                "single".to_string(),
                "feature".to_string(),
                "fix".to_string(),
                "governed".to_string(),
            ],
        }
    }

    fn both_templates() -> Vec<TemplateEntry> {
        vec![phase_template(), goal_template()]
    }

    // ── extract_intent ────────────────────────────────────────────────────────

    #[test]
    fn extracts_version_ref() {
        let intent = extract_intent("implement remaining v0.15 phases");
        assert_eq!(intent.version_ref.as_deref(), Some("v0.15"));
    }

    #[test]
    fn extracts_full_version_ref() {
        let intent = extract_intent("run v0.15.24 now");
        assert_eq!(intent.version_ref.as_deref(), Some("v0.15.24"));
    }

    #[test]
    fn extracts_intent_verb_implement() {
        let intent = extract_intent("implement remaining v0.15");
        assert_eq!(intent.intent_verb, "implement");
    }

    #[test]
    fn extracts_intent_verb_run() {
        let intent = extract_intent("run next phase");
        assert_eq!(intent.intent_verb, "run");
    }

    #[test]
    fn extracts_scope_remaining() {
        let intent = extract_intent("implement remaining v0.15");
        assert_eq!(intent.scope_modifier, "remaining");
    }

    #[test]
    fn extracts_scope_next() {
        let intent = extract_intent("run next phase");
        assert_eq!(intent.scope_modifier, "next");
    }

    #[test]
    fn detects_phase_mention() {
        let intent = extract_intent("run next phase");
        assert!(intent.mentions_phase);
    }

    #[test]
    fn no_phase_mention_without_word() {
        let intent = extract_intent("implement remaining v0.15");
        assert!(!intent.mentions_phase);
    }

    #[test]
    fn empty_text_yields_defaults() {
        let intent = extract_intent("do something");
        assert!(intent.version_ref.is_none());
        assert_eq!(intent.scope_modifier, "");
    }

    // ── score_template ────────────────────────────────────────────────────────

    #[test]
    fn implement_remaining_version_scores_high_on_phase_template() {
        let intent = extract_intent("implement remaining v0.15");
        let score = score_template(&phase_template(), &intent);
        assert!(
            score >= 0.80,
            "expected ≥ 0.80 for plan-build-phases, got {:.2}",
            score
        );
    }

    #[test]
    fn implement_remaining_version_scores_low_on_goal_template() {
        let intent = extract_intent("implement remaining v0.15");
        let score = score_template(&goal_template(), &intent);
        assert!(
            score < 0.80,
            "expected < 0.80 for governed-goal, got {:.2}",
            score
        );
    }

    #[test]
    fn run_next_phase_scores_high_on_phase_template() {
        let intent = extract_intent("run next phase");
        let score = score_template(&phase_template(), &intent);
        assert!(
            score >= 0.80,
            "expected ≥ 0.80 for plan-build-phases, got {:.2}",
            score
        );
    }

    #[test]
    fn run_next_phase_scores_lower_on_goal_template() {
        let run_next_intent = extract_intent("run next phase");
        let phase_score = score_template(&phase_template(), &run_next_intent);
        let goal_score = score_template(&goal_template(), &run_next_intent);
        assert!(
            phase_score > goal_score,
            "plan-build-phases ({:.2}) should outscore governed-goal ({:.2})",
            phase_score,
            goal_score
        );
    }

    #[test]
    fn low_signal_input_scores_below_threshold() {
        let intent = extract_intent("do something");
        let p = score_template(&phase_template(), &intent);
        let g = score_template(&goal_template(), &intent);
        assert!(
            p < 0.80 && g < 0.80,
            "expected both < 0.80, got {:.2} and {:.2}",
            p,
            g
        );
    }

    // ── resolve_intent ────────────────────────────────────────────────────────

    #[test]
    fn implement_remaining_resolves_to_plan_build_phases() {
        let plan_ctx = PlanContext {
            current_version_prefix: "v0.15".to_string(),
            next_pending_phase: "v0.15.24".to_string(),
            next_pending_title: "Intent Resolver".to_string(),
            pending_count: 3,
        };
        let templates = both_templates();
        let result = resolve_intent("implement remaining v0.15", &templates, &plan_ctx);
        match result {
            IntentResolution::Resolved { candidate, .. } => {
                assert_eq!(candidate.name, "plan-build-phases");
                assert!(
                    candidate
                        .params
                        .iter()
                        .any(|p| p.contains("phase_filter=v0.15")),
                    "expected phase_filter=v0.15 in params, got {:?}",
                    candidate.params
                );
            }
            IntentResolution::NeedsQuestion(q) => {
                panic!("expected Resolved, got NeedsQuestion: {}", q);
            }
        }
    }

    #[test]
    fn run_next_phase_resolves_to_plan_build_phases() {
        let plan_ctx = PlanContext {
            current_version_prefix: "v0.15".to_string(),
            next_pending_phase: "v0.15.24".to_string(),
            next_pending_title: "Intent Resolver".to_string(),
            pending_count: 3,
        };
        let templates = both_templates();
        let result = resolve_intent("run next phase", &templates, &plan_ctx);
        match result {
            IntentResolution::Resolved { candidate, .. } => {
                assert_eq!(candidate.name, "plan-build-phases");
            }
            IntentResolution::NeedsQuestion(q) => {
                panic!("expected Resolved, got NeedsQuestion: {}", q);
            }
        }
    }

    #[test]
    fn low_confidence_returns_clarifying_question() {
        let plan_ctx = PlanContext::default();
        let templates = both_templates();
        let result = resolve_intent("do something please", &templates, &plan_ctx);
        match result {
            IntentResolution::NeedsQuestion(q) => {
                assert!(!q.is_empty(), "clarifying question should not be empty");
            }
            IntentResolution::Resolved { candidate, .. } => {
                panic!(
                    "expected NeedsQuestion, got Resolved: {} ({:.2})",
                    candidate.name, candidate.score
                );
            }
        }
    }

    #[test]
    fn confirmation_card_contains_numbered_options() {
        let plan_ctx = PlanContext {
            current_version_prefix: "v0.15".to_string(),
            ..Default::default()
        };
        let templates = both_templates();
        let result = resolve_intent("implement remaining v0.15", &templates, &plan_ctx);
        match result {
            IntentResolution::Resolved {
                confirmation_card, ..
            } => {
                assert!(
                    confirmation_card.contains("1. Run"),
                    "card should contain numbered options"
                );
                assert!(confirmation_card.contains("4. Cancel"));
            }
            _ => panic!("expected Resolved"),
        }
    }

    #[test]
    fn derive_params_uses_version_ref() {
        let entry = phase_template();
        let intent = ExtractedIntent {
            version_ref: Some("v0.15".to_string()),
            intent_verb: "implement".to_string(),
            scope_modifier: "remaining".to_string(),
            mentions_phase: false,
        };
        let plan_ctx = PlanContext::default();
        let params = derive_params(&entry, &intent, &plan_ctx);
        assert!(
            params.iter().any(|p| p == "phase_filter=v0.15"),
            "expected phase_filter=v0.15, got {:?}",
            params
        );
    }

    #[test]
    fn derive_params_uses_plan_context_when_no_version() {
        let entry = phase_template();
        let intent = ExtractedIntent {
            version_ref: None,
            intent_verb: "run".to_string(),
            scope_modifier: "next".to_string(),
            mentions_phase: true,
        };
        let plan_ctx = PlanContext {
            current_version_prefix: "v0.15".to_string(),
            next_pending_phase: "v0.15.24".to_string(),
            ..Default::default()
        };
        let params = derive_params(&entry, &intent, &plan_ctx);
        assert!(
            params.iter().any(|p| p.contains("phase_filter=")),
            "expected phase_filter param, got {:?}",
            params
        );
    }
}
