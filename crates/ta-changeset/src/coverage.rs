//! Item coverage checker for Draft Pre-Apply Plan Review.
//!
//! For each unchecked `[ ]` item in the phase being applied, extracts 2-4
//! significant tokens and checks whether any appear in the draft artifact diffs.
//! Heuristic only — never blocks apply.

use crate::review_report::CoverageGap;

/// Extract 2-4 significant tokens from a plan item description.
/// Filters out common English stop-words and short tokens.
fn extract_tokens(text: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "an", "the", "and", "or", "in", "on", "at", "to", "for", "of", "with", "by", "from",
        "as", "is", "it", "its", "be", "this", "that", "are", "was", "will", "should", "when",
        "if", "all", "any", "not", "no", "so", "do", "new", "each", "per", "via", "into", "add",
        "use", "run", "get", "set", "has", "can", "may", "must", "after", "before", "up", "out",
        "into", "over", "then", "than",
    ];

    let tokens: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| {
            let lower = t.to_lowercase();
            t.len() >= 4 && !STOP_WORDS.contains(&lower.as_str())
        })
        .map(|t| t.to_lowercase())
        .collect();

    // Deduplicate while preserving order.
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

/// Check item coverage across a combined diff string.
///
/// `phase_id` — the phase ID (e.g., "v0.15.19.3")
/// `items` — list of `(item_number, item_text)` for unchecked items in that phase
/// `diff_content` — concatenated unified diffs for all artifacts in the draft
pub fn check_coverage(
    phase_id: &str,
    items: &[(usize, &str)],
    diff_content: &str,
) -> Vec<CoverageGap> {
    let diff_lower = diff_content.to_lowercase();

    items
        .iter()
        .filter_map(|(item_number, text)| {
            let tokens = extract_tokens(text);
            if tokens.is_empty() {
                // No useful tokens — can't assess coverage.
                return None;
            }

            let found = tokens.iter().any(|tok| diff_lower.contains(tok.as_str()));
            if found {
                None
            } else {
                let excerpt: String = text.chars().take(80).collect();
                Some(CoverageGap {
                    phase_id: phase_id.to_string(),
                    item_number: *item_number,
                    text_excerpt: excerpt,
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_found_in_diff_no_gap() {
        let items = vec![(1usize, "Add JsonFileStore persistence layer")];
        let diff = "--- a/src/store.rs\n+++ b/src/store.rs\n+impl JsonFileStore {\n";
        let gaps = check_coverage("v0.1.0", &items, diff);
        assert!(gaps.is_empty(), "expected no gaps, got {:?}", gaps);
    }

    #[test]
    fn token_absent_produces_gap() {
        let items = vec![(1usize, "Implement FancyAlgorithm with caching")];
        let diff = "--- a/src/other.rs\n+++ b/src/other.rs\n+fn nothing_related() {}\n";
        let gaps = check_coverage("v0.1.0", &items, diff);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].item_number, 1);
    }

    #[test]
    fn multiple_items_mixed_coverage() {
        let items = vec![
            (1usize, "Add AuthMiddleware struct"),
            (2usize, "Implement FancyWidget rendering"),
        ];
        let diff = "--- a/src/auth.rs\n+impl AuthMiddleware {}\n";
        let gaps = check_coverage("v0.1.0", &items, diff);
        // FancyWidget not found → 1 gap
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].item_number, 2);
    }
}
