// exemption.rs — Configurable summary exemption patterns (v0.4.0).
//
// Replaces the hardcoded `is_auto_summary_exempt()` function in draft.rs
// with a `.gitignore`-style pattern file (`.ta/summary-exempt`).
//
// Patterns match against `fs://workspace/` URIs. The file supports:
// - Glob patterns (e.g., `*.lock`, `**/*.toml`)
// - Comments (lines starting with `#`)
// - Blank lines (ignored)
//
// Default patterns are provided when no file exists, matching the
// previously hardcoded list (lockfiles, config manifests, docs).

use glob::Pattern;

/// A set of summary exemption patterns loaded from `.ta/summary-exempt`.
///
/// Files matching these patterns are exempt from summary enforcement —
/// they get auto-summaries and don't require agent-provided descriptions.
#[derive(Debug, Clone)]
pub struct ExemptionPatterns {
    patterns: Vec<Pattern>,
    raw_patterns: Vec<String>,
}

impl ExemptionPatterns {
    /// Load patterns from a file. Each non-empty, non-comment line is a glob pattern.
    pub fn from_file(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::parse_content(&content))
    }

    /// Parse patterns from a string (the file contents).
    pub fn parse_content(content: &str) -> Self {
        let mut patterns = Vec::new();
        let mut raw_patterns = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            // Skip comments and blank lines.
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            raw_patterns.push(trimmed.to_string());
            if let Ok(pattern) = Pattern::new(trimmed) {
                patterns.push(pattern);
            }
        }

        Self {
            patterns,
            raw_patterns,
        }
    }

    /// Load from file if it exists, otherwise use defaults.
    pub fn load_or_default(path: &std::path::Path) -> Self {
        if path.exists() {
            Self::from_file(path).unwrap_or_else(|_| Self::defaults())
        } else {
            Self::defaults()
        }
    }

    /// Default exemption patterns — matches the previously hardcoded list.
    pub fn defaults() -> Self {
        Self::parse_content(DEFAULT_PATTERNS)
    }

    /// Check if a URI is exempt from summary enforcement.
    ///
    /// The URI is expected to be in `fs://workspace/...` format.
    /// The path portion (after `fs://workspace/`) is matched against patterns.
    pub fn is_exempt(&self, uri: &str) -> bool {
        let path = uri.strip_prefix("fs://workspace/").unwrap_or(uri);
        self.patterns.iter().any(|p| {
            p.matches(path)
                || path
                    .rsplit('/')
                    .next()
                    .map(|filename| p.matches(filename))
                    .unwrap_or(false)
        })
    }

    /// Return the raw pattern strings (for display/debugging).
    pub fn raw_patterns(&self) -> &[String] {
        &self.raw_patterns
    }
}

/// Default exemption patterns — equivalent to the hardcoded list from v0.3.6.
const DEFAULT_PATTERNS: &str = r#"# Default summary exemption patterns.
# Files matching these patterns get auto-summaries and don't need
# agent-provided descriptions at `ta draft build` time.
#
# Format: .gitignore-style glob patterns, one per line.
# Matches against the path portion of fs://workspace/ URIs.

# Lockfiles
Cargo.lock
package-lock.json
yarn.lock
pnpm-lock.yaml
Gemfile.lock
poetry.lock

# Config / manifest files
Cargo.toml
package.json
pyproject.toml

# Plan / docs
PLAN.md
CHANGELOG.md
README.md
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_patterns_match_lockfiles() {
        let patterns = ExemptionPatterns::defaults();
        assert!(patterns.is_exempt("fs://workspace/Cargo.lock"));
        assert!(patterns.is_exempt("fs://workspace/package-lock.json"));
        assert!(patterns.is_exempt("fs://workspace/yarn.lock"));
        assert!(patterns.is_exempt("fs://workspace/pnpm-lock.yaml"));
        assert!(patterns.is_exempt("fs://workspace/Gemfile.lock"));
        assert!(patterns.is_exempt("fs://workspace/poetry.lock"));
    }

    #[test]
    fn default_patterns_match_config_manifests() {
        let patterns = ExemptionPatterns::defaults();
        assert!(patterns.is_exempt("fs://workspace/Cargo.toml"));
        assert!(patterns.is_exempt("fs://workspace/package.json"));
        assert!(patterns.is_exempt("fs://workspace/pyproject.toml"));
    }

    #[test]
    fn default_patterns_match_nested_config_files() {
        let patterns = ExemptionPatterns::defaults();
        // Should match Cargo.toml in subdirectories via filename matching
        assert!(patterns.is_exempt("fs://workspace/crates/foo/Cargo.toml"));
        assert!(patterns.is_exempt("fs://workspace/deep/pnpm-lock.yaml"));
    }

    #[test]
    fn default_patterns_match_docs() {
        let patterns = ExemptionPatterns::defaults();
        assert!(patterns.is_exempt("fs://workspace/PLAN.md"));
        assert!(patterns.is_exempt("fs://workspace/CHANGELOG.md"));
        assert!(patterns.is_exempt("fs://workspace/README.md"));
    }

    #[test]
    fn default_patterns_do_not_match_source_files() {
        let patterns = ExemptionPatterns::defaults();
        assert!(!patterns.is_exempt("fs://workspace/src/main.rs"));
        assert!(!patterns.is_exempt("fs://workspace/src/lib.rs"));
        assert!(!patterns.is_exempt("fs://workspace/tests/test.rs"));
        assert!(!patterns.is_exempt("fs://workspace/build.rs"));
    }

    #[test]
    fn custom_patterns_override_defaults() {
        let content = "*.lock\n*.md\n";
        let patterns = ExemptionPatterns::parse_content(content);
        assert!(patterns.is_exempt("fs://workspace/Cargo.lock"));
        assert!(patterns.is_exempt("fs://workspace/README.md"));
        // *.toml is NOT in the custom patterns
        assert!(!patterns.is_exempt("fs://workspace/Cargo.toml"));
    }

    #[test]
    fn comments_and_blanks_are_ignored() {
        let content = "# This is a comment\n\n*.lock\n  # Another comment\n";
        let patterns = ExemptionPatterns::parse_content(content);
        assert_eq!(patterns.raw_patterns().len(), 1);
        assert!(patterns.is_exempt("fs://workspace/Cargo.lock"));
    }

    #[test]
    fn glob_star_patterns() {
        let content = "**/*.generated.*\n";
        let patterns = ExemptionPatterns::parse_content(content);
        assert!(patterns.is_exempt("fs://workspace/src/types.generated.ts"));
        assert!(patterns.is_exempt("fs://workspace/deep/path/schema.generated.rs"));
        assert!(!patterns.is_exempt("fs://workspace/src/main.rs"));
    }

    #[test]
    fn empty_patterns_exempt_nothing() {
        let patterns = ExemptionPatterns::parse_content("");
        assert!(!patterns.is_exempt("fs://workspace/anything.rs"));
        assert!(!patterns.is_exempt("fs://workspace/Cargo.lock"));
    }

    #[test]
    fn uri_without_prefix_still_matches() {
        let patterns = ExemptionPatterns::defaults();
        // If URI doesn't have the prefix, match against the raw string
        assert!(patterns.is_exempt("Cargo.lock"));
    }

    #[test]
    fn load_or_default_returns_defaults_for_missing_file() {
        let patterns =
            ExemptionPatterns::load_or_default(std::path::Path::new("/nonexistent/path"));
        assert!(patterns.is_exempt("fs://workspace/Cargo.lock"));
    }

    #[test]
    fn load_from_tempfile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("summary-exempt");
        std::fs::write(&path, "*.custom\nspecial.txt\n").unwrap();

        let patterns = ExemptionPatterns::from_file(&path).unwrap();
        assert!(patterns.is_exempt("fs://workspace/file.custom"));
        assert!(patterns.is_exempt("fs://workspace/special.txt"));
        assert!(!patterns.is_exempt("fs://workspace/src/main.rs"));
    }

    #[test]
    fn raw_patterns_accessible() {
        let patterns = ExemptionPatterns::parse_content("*.lock\n*.toml\n");
        assert_eq!(patterns.raw_patterns().len(), 2);
        assert_eq!(patterns.raw_patterns()[0], "*.lock");
        assert_eq!(patterns.raw_patterns()[1], "*.toml");
    }
}
