//! explanation.rs — Parser for .diff.explanation.yaml sidecar files (v0.2.3).
//!
//! Agents write explanation sidecars alongside changes to provide tiered
//! explanations for reviewers: summary → explanation → full diff.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ChangeSetError;
use crate::pr_package::ExplanationTiers;

/// Schema for .diff.explanation.yaml sidecar files.
///
/// Example YAML:
/// ```yaml
/// file: src/auth/middleware.rs
/// summary: "Refactored auth middleware to use JWT instead of session tokens"
/// explanation: |
///   Replaced session-based auth with JWT validation. The middleware now
///   checks the Authorization header for a Bearer token, validates it
///   against the JWKS endpoint, and extracts claims into the request context.
///   This change touches 3 files: middleware.rs (core logic), config.rs
///   (JWT settings), and tests/auth_test.rs (updated test fixtures).
/// tags: [security, breaking-change]
/// related_artifacts:
///   - src/auth/config.rs
///   - tests/auth_test.rs
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExplanationSidecar {
    /// The file this explanation applies to (relative path from workspace root).
    pub file: String,
    /// One-line summary.
    pub summary: String,
    /// Multi-line explanation of what changed and why.
    pub explanation: String,
    /// Optional tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Related artifacts (paths relative to workspace root).
    #[serde(default)]
    pub related_artifacts: Vec<String>,
}

impl ExplanationSidecar {
    /// Parse an explanation sidecar from a YAML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ChangeSetError> {
        let contents = fs::read_to_string(path.as_ref()).map_err(|e| {
            ChangeSetError::InvalidData(format!(
                "Failed to read explanation sidecar at {}: {}",
                path.as_ref().display(),
                e
            ))
        })?;

        serde_yaml::from_str(&contents).map_err(|e| {
            ChangeSetError::InvalidData(format!(
                "Failed to parse explanation sidecar YAML at {}: {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// Convert this sidecar into ExplanationTiers (for embedding in Artifact).
    ///
    /// Normalizes related_artifacts to URI format (fs://workspace/<path>).
    pub fn into_tiers(self) -> ExplanationTiers {
        ExplanationTiers {
            summary: self.summary,
            explanation: self.explanation,
            tags: self.tags,
            related_artifacts: self
                .related_artifacts
                .into_iter()
                .map(|path| {
                    if path.starts_with("fs://") {
                        path
                    } else {
                        format!("fs://workspace/{}", path.trim_start_matches('/'))
                    }
                })
                .collect(),
        }
    }

    /// Find explanation sidecar for a given file path.
    ///
    /// Looks for: `<file_path>.diff.explanation.yaml`
    ///
    /// Returns None if the sidecar doesn't exist (this is not an error —
    /// sidecars are optional).
    pub fn find_for_file<P: AsRef<Path>>(file_path: P) -> Option<Self> {
        let sidecar_path = format!("{}.diff.explanation.yaml", file_path.as_ref().display());
        if Path::new(&sidecar_path).exists() {
            Self::from_file(sidecar_path).ok()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parse_valid_yaml() {
        let yaml = r#"
file: src/auth/middleware.rs
summary: "Refactored auth middleware to use JWT"
explanation: |
  Replaced session-based auth with JWT validation.
  This improves security and scalability.
tags:
  - security
  - breaking-change
related_artifacts:
  - src/auth/config.rs
  - tests/auth_test.rs
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        let sidecar = ExplanationSidecar::from_file(file.path()).unwrap();
        assert_eq!(sidecar.file, "src/auth/middleware.rs");
        assert_eq!(sidecar.summary, "Refactored auth middleware to use JWT");
        assert!(sidecar.explanation.contains("JWT validation"));
        assert_eq!(sidecar.tags.len(), 2);
        assert_eq!(sidecar.related_artifacts.len(), 2);
    }

    #[test]
    fn parse_minimal_yaml() {
        let yaml = r#"
file: test.txt
summary: "Added test file"
explanation: "This is a test file for validation."
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        let sidecar = ExplanationSidecar::from_file(file.path()).unwrap();
        assert_eq!(sidecar.file, "test.txt");
        assert!(sidecar.tags.is_empty());
        assert!(sidecar.related_artifacts.is_empty());
    }

    #[test]
    fn into_tiers_normalizes_uris() {
        let sidecar = ExplanationSidecar {
            file: "src/main.rs".to_string(),
            summary: "Test".to_string(),
            explanation: "Test explanation".to_string(),
            tags: vec![],
            related_artifacts: vec![
                "src/lib.rs".to_string(),
                "fs://workspace/tests/test.rs".to_string(),
            ],
        };

        let tiers = sidecar.into_tiers();
        assert_eq!(tiers.related_artifacts.len(), 2);
        assert_eq!(tiers.related_artifacts[0], "fs://workspace/src/lib.rs");
        assert_eq!(tiers.related_artifacts[1], "fs://workspace/tests/test.rs");
    }

    #[test]
    fn find_for_file_returns_none_when_missing() {
        let result = ExplanationSidecar::find_for_file("/nonexistent/file.rs");
        assert!(result.is_none());
    }

    #[test]
    fn find_for_file_returns_sidecar_when_present() {
        let yaml = r#"
file: test.txt
summary: "Test"
explanation: "Test explanation"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        // Create sidecar with expected naming convention
        let base_path = file.path().parent().unwrap().join("test_file.rs");
        let sidecar_path = format!("{}.diff.explanation.yaml", base_path.display());
        fs::write(&sidecar_path, yaml).unwrap();

        let result = ExplanationSidecar::find_for_file(&base_path);
        assert!(result.is_some());
        assert_eq!(result.unwrap().summary, "Test");

        // Cleanup
        fs::remove_file(&sidecar_path).ok();
    }

    #[test]
    fn invalid_yaml_returns_error() {
        let yaml = "this is not valid yaml: [unclosed";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        let result = ExplanationSidecar::from_file(file.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse explanation sidecar YAML"));
    }
}
