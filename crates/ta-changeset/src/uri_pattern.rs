// uri_pattern.rs — URI-aware pattern matching for selective approval.
//
// Matches artifact resource_uri values against user-provided patterns.
// Safety-first: patterns are scoped by URI scheme so filesystem globs
// can never accidentally match email or database URIs.
//
// Examples:
//   "src/**"                → matches "fs://workspace/src/main.rs" (auto-prefixed)
//   "fs://workspace/src/**" → matches "fs://workspace/src/main.rs" (explicit)
//   "gmail://*"             → matches "gmail://inbox/msg-123" (email scheme)
//   "src/**"                → does NOT match "gmail://inbox/src/draft" (scheme mismatch)

use glob::{MatchOptions, Pattern};

/// The default URI scheme + authority prefix for bare patterns.
const FS_PREFIX: &str = "fs://workspace/";

/// Match a pattern against a resource URI with scheme-awareness.
///
/// Rules:
/// 1. If pattern has a scheme (`://`), match the full URI as a glob.
/// 2. If pattern has no scheme (bare path like `src/**`), auto-prefix with
///    `fs://workspace/` and only match `fs://` URIs.
/// 3. Scheme mismatch = no match (safety invariant).
/// 4. Invalid glob patterns never match (fail-closed).
pub fn matches_uri(pattern: &str, uri: &str) -> bool {
    if pattern.contains("://") {
        // Explicit scheme — extract and compare schemes before globbing.
        let pattern_scheme = scheme_of(pattern);
        let uri_scheme = scheme_of(uri);
        if pattern_scheme != uri_scheme {
            return false;
        }
        glob_match(pattern, uri)
    } else {
        // Bare pattern — only match fs:// URIs.
        if !uri.starts_with(FS_PREFIX) {
            return false;
        }
        let full_pattern = format!("{}{}", FS_PREFIX, pattern);
        glob_match(&full_pattern, uri)
    }
}

/// Extract the scheme portion of a URI (everything before `://`).
fn scheme_of(uri: &str) -> &str {
    uri.split("://").next().unwrap_or("")
}

/// Glob-match a pattern against a target string. Fail-closed on invalid patterns.
fn glob_match(pattern: &str, target: &str) -> bool {
    let opts = MatchOptions {
        require_literal_separator: true,
        ..Default::default()
    };
    match Pattern::new(pattern) {
        Ok(p) => p.matches_with(target, opts),
        Err(_) => false,
    }
}

/// Resolve a user-provided pattern into its full URI form.
/// Useful for displaying what a pattern actually matches.
pub fn resolve_pattern(pattern: &str) -> String {
    if pattern.contains("://") {
        pattern.to_string()
    } else {
        format!("{}{}", FS_PREFIX, pattern)
    }
}

/// Filter a list of URIs by a set of patterns. Returns matching URIs.
pub fn filter_uris<'a>(patterns: &[&str], uris: &[&'a str]) -> Vec<&'a str> {
    uris.iter()
        .filter(|uri| patterns.iter().any(|pat| matches_uri(pat, uri)))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Bare patterns (auto-prefix fs://workspace/) ──────────────

    #[test]
    fn bare_pattern_matches_fs_uri() {
        assert!(matches_uri("src/**", "fs://workspace/src/main.rs"));
        assert!(matches_uri("src/*.rs", "fs://workspace/src/lib.rs"));
        assert!(matches_uri("README.md", "fs://workspace/README.md"));
    }

    #[test]
    fn bare_pattern_does_not_match_other_schemes() {
        // Safety: bare "src/**" must NOT match gmail URIs.
        assert!(!matches_uri("src/**", "gmail://inbox/src/draft"));
        assert!(!matches_uri("src/**", "drive://docs/src/readme"));
        assert!(!matches_uri("*.rs", "db://tables/schema.rs"));
    }

    #[test]
    fn bare_pattern_no_match_outside_scope() {
        assert!(!matches_uri("src/**", "fs://workspace/tests/test.rs"));
        assert!(!matches_uri("src/*.rs", "fs://workspace/src/sub/deep.rs"));
    }

    // ── Explicit scheme patterns ─────────────────────────────────

    #[test]
    fn explicit_fs_pattern_matches() {
        assert!(matches_uri(
            "fs://workspace/src/**",
            "fs://workspace/src/main.rs"
        ));
        assert!(matches_uri(
            "fs://workspace/**",
            "fs://workspace/Cargo.toml"
        ));
    }

    #[test]
    fn explicit_gmail_pattern_matches() {
        // Single * matches one path segment (require_literal_separator).
        assert!(matches_uri("gmail://inbox/*", "gmail://inbox/msg-456"));
        // ** matches across path separators.
        assert!(matches_uri("gmail://**", "gmail://inbox/msg-123"));
        // Single * does NOT cross path separators.
        assert!(!matches_uri("gmail://*", "gmail://inbox/msg-123"));
    }

    #[test]
    fn scheme_mismatch_never_matches() {
        // fs:// pattern vs gmail:// URI.
        assert!(!matches_uri("fs://workspace/**", "gmail://inbox/msg-123"));
        // gmail:// pattern vs fs:// URI.
        assert!(!matches_uri("gmail://*", "fs://workspace/src/main.rs"));
    }

    // ── Edge cases ───────────────────────────────────────────────

    #[test]
    fn exact_path_match() {
        assert!(matches_uri("src/main.rs", "fs://workspace/src/main.rs"));
        assert!(!matches_uri("src/main.rs", "fs://workspace/src/lib.rs"));
    }

    #[test]
    fn double_star_matches_deep_paths() {
        assert!(matches_uri(
            "src/**",
            "fs://workspace/src/deeply/nested/file.rs"
        ));
    }

    #[test]
    fn invalid_glob_pattern_never_matches() {
        // Unclosed bracket — should fail-closed.
        assert!(!matches_uri("[invalid", "fs://workspace/src/main.rs"));
    }

    #[test]
    fn empty_pattern_does_not_match() {
        assert!(!matches_uri("", "fs://workspace/src/main.rs"));
    }

    // ── Helpers ──────────────────────────────────────────────────

    #[test]
    fn resolve_bare_pattern() {
        assert_eq!(resolve_pattern("src/**"), "fs://workspace/src/**");
    }

    #[test]
    fn resolve_explicit_pattern_unchanged() {
        assert_eq!(resolve_pattern("gmail://*"), "gmail://*");
    }

    #[test]
    fn filter_uris_selects_matching() {
        let uris = vec![
            "fs://workspace/src/main.rs",
            "fs://workspace/src/lib.rs",
            "fs://workspace/tests/test.rs",
            "gmail://inbox/msg-1",
        ];
        let matched = filter_uris(&["src/**"], &uris);
        assert_eq!(matched.len(), 2);
        assert!(matched.contains(&"fs://workspace/src/main.rs"));
        assert!(matched.contains(&"fs://workspace/src/lib.rs"));
    }

    #[test]
    fn filter_uris_multiple_patterns() {
        let uris = vec![
            "fs://workspace/src/main.rs",
            "fs://workspace/tests/test.rs",
            "gmail://inbox/msg-1",
        ];
        let matched = filter_uris(&["src/**", "gmail://**"], &uris);
        assert_eq!(matched.len(), 2);
        assert!(matched.contains(&"fs://workspace/src/main.rs"));
        assert!(matched.contains(&"gmail://inbox/msg-1"));
    }
}
