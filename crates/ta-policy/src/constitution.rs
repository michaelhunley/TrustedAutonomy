// constitution.rs — Per-goal Access Constitutions (v0.4.3).
//
// An access constitution declares what URIs an agent should need to access
// to complete a given goal. It serves as a pre-declared intent contract —
// any deviation is a behavioral drift signal.
//
// Relationship to v0.4.0 alignment profiles:
// - AlignmentProfile describes an agent's *general* capability envelope
// - AccessConstitution is *per-goal* — scoped to a specific task
//
// Standards alignment:
// - IEEE 3152-2024: Pre-declared intent satisfies transparency requirements
// - NIST AI RMF GOVERN 1.4: Documented processes mapping AI behavior to purpose
// - EU AI Act Article 14: Human oversight via reviewable, pre-approved scope

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A per-goal access constitution declaring what URIs an agent should access.
///
/// Stored as YAML at `.ta/constitutions/goal-<id>.yaml`.
///
/// ```yaml
/// goal_id: "abc-123"
/// created_by: "human"
/// access:
///   - pattern: "fs://workspace/src/commands/draft.rs"
///     intent: "Add summary enforcement logic"
///   - pattern: "fs://workspace/crates/ta-submit/src/config.rs"
///     intent: "Add BuildConfig struct"
/// enforcement: warning
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessConstitution {
    /// The goal this constitution is scoped to.
    pub goal_id: String,

    /// Who created this constitution ("human", "ta-supervisor", agent ID).
    pub created_by: String,

    /// When this constitution was created.
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// The declared access patterns with intent annotations.
    pub access: Vec<ConstitutionEntry>,

    /// Enforcement mode: "warning" (default) or "error" (strict).
    #[serde(default = "default_enforcement")]
    pub enforcement: EnforcementMode,
}

/// A single access declaration within a constitution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstitutionEntry {
    /// URI pattern (glob) the agent is expected to access.
    /// Can be bare (e.g., "src/commands/draft.rs") or fully qualified
    /// (e.g., "fs://workspace/src/commands/draft.rs").
    pub pattern: String,

    /// Why the agent needs access to this resource.
    pub intent: String,
}

/// How strictly constitution violations are enforced at `ta draft build` time.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementMode {
    /// Undeclared access prints a warning but build proceeds.
    #[default]
    Warning,
    /// Undeclared access causes the build to fail.
    Error,
}

impl std::fmt::Display for EnforcementMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnforcementMode::Warning => write!(f, "warning"),
            EnforcementMode::Error => write!(f, "error"),
        }
    }
}

fn default_enforcement() -> EnforcementMode {
    EnforcementMode::Warning
}

/// Result of validating artifacts against an access constitution.
#[derive(Debug, Clone)]
pub struct ConstitutionValidation {
    /// Artifact URIs that are declared in the constitution.
    pub declared: Vec<String>,
    /// Artifact URIs that are NOT declared in the constitution (violations).
    pub undeclared: Vec<String>,
    /// Constitution entries that were declared but no artifact matched.
    pub unused: Vec<ConstitutionEntry>,
    /// The enforcement mode from the constitution.
    pub enforcement: EnforcementMode,
}

impl ConstitutionValidation {
    /// Whether the validation passed (no undeclared access).
    pub fn passed(&self) -> bool {
        self.undeclared.is_empty()
    }
}

/// Validate artifact URIs against an access constitution.
///
/// Uses the same URI-pattern matching as selective approval (scheme-aware globs).
pub fn validate_constitution(
    constitution: &AccessConstitution,
    artifact_uris: &[&str],
) -> ConstitutionValidation {
    let mut declared = Vec::new();
    let mut undeclared = Vec::new();
    let mut entry_matched = vec![false; constitution.access.len()];

    for uri in artifact_uris {
        let mut matched = false;
        for (i, entry) in constitution.access.iter().enumerate() {
            if pattern_matches(&entry.pattern, uri) {
                matched = true;
                entry_matched[i] = true;
            }
        }
        if matched {
            declared.push(uri.to_string());
        } else {
            undeclared.push(uri.to_string());
        }
    }

    let unused: Vec<ConstitutionEntry> = constitution
        .access
        .iter()
        .enumerate()
        .filter(|(i, _)| !entry_matched[*i])
        .map(|(_, e)| e.clone())
        .collect();

    ConstitutionValidation {
        declared,
        undeclared,
        unused,
        enforcement: constitution.enforcement,
    }
}

/// Match a constitution pattern against a URI.
///
/// Supports:
/// - Exact paths: `src/main.rs` → auto-prefixed to `fs://workspace/src/main.rs`
/// - Globs: `src/**` → matches any file under `fs://workspace/src/`
/// - Full URIs: `fs://workspace/src/**` → explicit scheme matching
fn pattern_matches(pattern: &str, uri: &str) -> bool {
    // Delegate to the existing URI pattern matching in ta-changeset.
    // We reimplement the core logic here to avoid a cross-crate dependency
    // from ta-policy → ta-changeset, keeping the dependency graph clean.
    const FS_PREFIX: &str = "fs://workspace/";

    if pattern.contains("://") {
        // Explicit scheme — extract and compare schemes before globbing.
        let pattern_scheme = pattern.split("://").next().unwrap_or("");
        let uri_scheme = uri.split("://").next().unwrap_or("");
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

/// Glob-match with literal separator requirement.
fn glob_match(pattern: &str, target: &str) -> bool {
    let opts = glob::MatchOptions {
        require_literal_separator: true,
        ..Default::default()
    };
    match glob::Pattern::new(pattern) {
        Ok(p) => p.matches_with(target, opts),
        Err(_) => false,
    }
}

// ── Constitution Store ──

/// Reads and writes constitution YAML files from `.ta/constitutions/`.
pub struct ConstitutionStore {
    dir: PathBuf,
}

impl ConstitutionStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Construct a store from a workspace root (uses `.ta/constitutions/`).
    pub fn for_workspace(workspace_root: &Path) -> Self {
        Self::new(workspace_root.join(".ta").join("constitutions"))
    }

    /// Load a constitution for the given goal. Returns None if not found.
    pub fn load(&self, goal_id: &str) -> Result<Option<AccessConstitution>, ConstitutionError> {
        let path = self.path_for(goal_id);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path).map_err(|source| ConstitutionError::IoError {
            path: path.clone(),
            source,
        })?;
        let constitution: AccessConstitution =
            serde_yaml::from_str(&data).map_err(ConstitutionError::ParseError)?;
        Ok(Some(constitution))
    }

    /// Save a constitution for a goal.
    pub fn save(&self, constitution: &AccessConstitution) -> Result<(), ConstitutionError> {
        fs::create_dir_all(&self.dir).map_err(|source| ConstitutionError::IoError {
            path: self.dir.clone(),
            source,
        })?;
        let path = self.path_for(&constitution.goal_id);
        let yaml =
            serde_yaml::to_string(constitution).map_err(ConstitutionError::SerializeError)?;
        fs::write(&path, yaml).map_err(|source| ConstitutionError::IoError { path, source })?;
        Ok(())
    }

    /// List all goal IDs that have constitutions.
    pub fn list_goals(&self) -> Result<Vec<String>, ConstitutionError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut goals = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(|source| ConstitutionError::IoError {
            path: self.dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| ConstitutionError::IoError {
                path: self.dir.clone(),
                source,
            })?;
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Strip "goal-" prefix if present.
                    let id = stem.strip_prefix("goal-").unwrap_or(stem);
                    goals.push(id.to_string());
                }
            }
        }
        goals.sort();
        Ok(goals)
    }

    /// Get the file path for a goal's constitution.
    fn path_for(&self, goal_id: &str) -> PathBuf {
        self.dir.join(format!("goal-{}.yaml", goal_id))
    }
}

/// Errors from constitution operations.
#[derive(Debug, thiserror::Error)]
pub enum ConstitutionError {
    #[error("I/O error at {path}: {source}")]
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse constitution YAML: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("failed to serialize constitution: {0}")]
    SerializeError(serde_yaml::Error),
}

// ── Proposal helper ──

/// Propose an access constitution from a goal's objective and historical patterns.
///
/// This is a simple heuristic implementation — a more sophisticated version
/// would use the agent's historical access patterns from BaselineStore.
pub fn propose_constitution(
    goal_id: &str,
    _goal_objective: &str,
    historical_patterns: &[String],
) -> AccessConstitution {
    let access: Vec<ConstitutionEntry> = historical_patterns
        .iter()
        .map(|pattern| ConstitutionEntry {
            pattern: format!("{}**", pattern),
            intent: format!("Historical access pattern: {}", pattern),
        })
        .collect();

    AccessConstitution {
        goal_id: goal_id.to_string(),
        created_by: "ta-supervisor".to_string(),
        created_at: Utc::now(),
        access,
        enforcement: EnforcementMode::Warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AccessConstitution serialization ──

    #[test]
    fn constitution_yaml_round_trip() {
        let constitution = AccessConstitution {
            goal_id: "abc-123".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![
                ConstitutionEntry {
                    pattern: "src/commands/draft.rs".to_string(),
                    intent: "Add summary enforcement logic".to_string(),
                },
                ConstitutionEntry {
                    pattern: "crates/ta-submit/src/config.rs".to_string(),
                    intent: "Add BuildConfig struct".to_string(),
                },
            ],
            enforcement: EnforcementMode::Warning,
        };

        let yaml = serde_yaml::to_string(&constitution).unwrap();
        let restored: AccessConstitution = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(constitution.goal_id, restored.goal_id);
        assert_eq!(constitution.created_by, restored.created_by);
        assert_eq!(constitution.access.len(), restored.access.len());
        assert_eq!(constitution.enforcement, restored.enforcement);
    }

    #[test]
    fn constitution_json_round_trip() {
        let constitution = AccessConstitution {
            goal_id: "def-456".to_string(),
            created_by: "ta-supervisor".to_string(),
            created_at: Utc::now(),
            access: vec![ConstitutionEntry {
                pattern: "fs://workspace/src/**".to_string(),
                intent: "Full source access".to_string(),
            }],
            enforcement: EnforcementMode::Error,
        };

        let json = serde_json::to_string(&constitution).unwrap();
        let restored: AccessConstitution = serde_json::from_str(&json).unwrap();

        assert_eq!(constitution.goal_id, restored.goal_id);
        assert_eq!(restored.enforcement, EnforcementMode::Error);
    }

    #[test]
    fn enforcement_mode_defaults_to_warning() {
        let yaml = r#"
goal_id: "test"
created_by: "human"
access: []
"#;
        let constitution: AccessConstitution = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(constitution.enforcement, EnforcementMode::Warning);
    }

    #[test]
    fn enforcement_mode_display() {
        assert_eq!(EnforcementMode::Warning.to_string(), "warning");
        assert_eq!(EnforcementMode::Error.to_string(), "error");
    }

    // ── Validation ──

    #[test]
    fn validate_all_declared_passes() {
        let constitution = AccessConstitution {
            goal_id: "test".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![
                ConstitutionEntry {
                    pattern: "src/commands/**".to_string(),
                    intent: "Modify commands".to_string(),
                },
                ConstitutionEntry {
                    pattern: "src/lib.rs".to_string(),
                    intent: "Update module exports".to_string(),
                },
            ],
            enforcement: EnforcementMode::Warning,
        };

        let uris = vec![
            "fs://workspace/src/commands/draft.rs",
            "fs://workspace/src/commands/audit.rs",
            "fs://workspace/src/lib.rs",
        ];

        let result = validate_constitution(&constitution, &uris);
        assert!(result.passed());
        assert_eq!(result.declared.len(), 3);
        assert!(result.undeclared.is_empty());
    }

    #[test]
    fn validate_detects_undeclared_access() {
        let constitution = AccessConstitution {
            goal_id: "test".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![ConstitutionEntry {
                pattern: "src/commands/**".to_string(),
                intent: "Modify commands".to_string(),
            }],
            enforcement: EnforcementMode::Warning,
        };

        let uris = vec![
            "fs://workspace/src/commands/draft.rs",
            "fs://workspace/crates/ta-audit/src/drift.rs", // undeclared!
        ];

        let result = validate_constitution(&constitution, &uris);
        assert!(!result.passed());
        assert_eq!(result.declared.len(), 1);
        assert_eq!(result.undeclared.len(), 1);
        assert_eq!(
            result.undeclared[0],
            "fs://workspace/crates/ta-audit/src/drift.rs"
        );
    }

    #[test]
    fn validate_detects_unused_entries() {
        let constitution = AccessConstitution {
            goal_id: "test".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![
                ConstitutionEntry {
                    pattern: "src/commands/**".to_string(),
                    intent: "Modify commands".to_string(),
                },
                ConstitutionEntry {
                    pattern: "tests/**".to_string(),
                    intent: "Add tests".to_string(),
                },
            ],
            enforcement: EnforcementMode::Warning,
        };

        let uris = vec!["fs://workspace/src/commands/draft.rs"];

        let result = validate_constitution(&constitution, &uris);
        assert!(result.passed()); // No violations — unused entries are informational.
        assert_eq!(result.unused.len(), 1);
        assert_eq!(result.unused[0].pattern, "tests/**");
    }

    #[test]
    fn validate_explicit_uri_patterns() {
        let constitution = AccessConstitution {
            goal_id: "test".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![ConstitutionEntry {
                pattern: "fs://workspace/src/**".to_string(),
                intent: "Full source access".to_string(),
            }],
            enforcement: EnforcementMode::Error,
        };

        let uris = vec![
            "fs://workspace/src/main.rs",
            "fs://workspace/src/deeply/nested/file.rs",
        ];

        let result = validate_constitution(&constitution, &uris);
        assert!(result.passed());
        assert_eq!(result.enforcement, EnforcementMode::Error);
    }

    #[test]
    fn validate_scheme_mismatch_is_undeclared() {
        let constitution = AccessConstitution {
            goal_id: "test".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![ConstitutionEntry {
                pattern: "fs://workspace/src/**".to_string(),
                intent: "Source access".to_string(),
            }],
            enforcement: EnforcementMode::Warning,
        };

        // Gmail URI doesn't match an fs:// pattern.
        let uris = vec!["gmail://inbox/msg-123"];
        let result = validate_constitution(&constitution, &uris);
        assert!(!result.passed());
        assert_eq!(result.undeclared.len(), 1);
    }

    #[test]
    fn validate_empty_constitution_flags_everything() {
        let constitution = AccessConstitution {
            goal_id: "test".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![],
            enforcement: EnforcementMode::Warning,
        };

        let uris = vec!["fs://workspace/src/main.rs"];
        let result = validate_constitution(&constitution, &uris);
        assert!(!result.passed());
        assert_eq!(result.undeclared.len(), 1);
    }

    #[test]
    fn validate_empty_artifacts_passes() {
        let constitution = AccessConstitution {
            goal_id: "test".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![ConstitutionEntry {
                pattern: "src/**".to_string(),
                intent: "Source access".to_string(),
            }],
            enforcement: EnforcementMode::Warning,
        };

        let uris: Vec<&str> = vec![];
        let result = validate_constitution(&constitution, &uris);
        assert!(result.passed());
    }

    // ── ConstitutionStore ──

    #[test]
    fn store_save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConstitutionStore::new(dir.path().to_path_buf());

        let constitution = AccessConstitution {
            goal_id: "abc-123".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![ConstitutionEntry {
                pattern: "src/**".to_string(),
                intent: "Source access".to_string(),
            }],
            enforcement: EnforcementMode::Warning,
        };

        store.save(&constitution).unwrap();
        let loaded = store.load("abc-123").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.goal_id, "abc-123");
        assert_eq!(loaded.access.len(), 1);
    }

    #[test]
    fn store_load_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConstitutionStore::new(dir.path().to_path_buf());
        let result = store.load("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn store_list_goals() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConstitutionStore::new(dir.path().to_path_buf());

        let c1 = AccessConstitution {
            goal_id: "goal-alpha".to_string(),
            created_by: "human".to_string(),
            created_at: Utc::now(),
            access: vec![],
            enforcement: EnforcementMode::Warning,
        };
        let mut c2 = c1.clone();
        c2.goal_id = "goal-beta".to_string();

        store.save(&c1).unwrap();
        store.save(&c2).unwrap();

        let goals = store.list_goals().unwrap();
        assert_eq!(goals, vec!["goal-alpha", "goal-beta"]);
    }

    #[test]
    fn store_list_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConstitutionStore::new(dir.path().join("nonexistent"));
        let goals = store.list_goals().unwrap();
        assert!(goals.is_empty());
    }

    // ── Pattern matching ──

    #[test]
    fn pattern_matches_bare_path() {
        assert!(pattern_matches("src/main.rs", "fs://workspace/src/main.rs"));
        assert!(!pattern_matches("src/main.rs", "fs://workspace/src/lib.rs"));
    }

    #[test]
    fn pattern_matches_glob() {
        assert!(pattern_matches("src/**", "fs://workspace/src/main.rs"));
        assert!(pattern_matches(
            "src/**",
            "fs://workspace/src/deeply/nested.rs"
        ));
        assert!(!pattern_matches("src/**", "fs://workspace/tests/test.rs"));
    }

    #[test]
    fn pattern_matches_explicit_uri() {
        assert!(pattern_matches(
            "fs://workspace/src/**",
            "fs://workspace/src/main.rs"
        ));
        assert!(!pattern_matches(
            "fs://workspace/src/**",
            "gmail://inbox/msg-1"
        ));
    }

    // ── Proposal ──

    #[test]
    fn propose_from_historical_patterns() {
        let patterns = vec![
            "fs://workspace/src/".to_string(),
            "fs://workspace/tests/".to_string(),
        ];
        let constitution = propose_constitution("goal-1", "Fix auth bug", &patterns);

        assert_eq!(constitution.goal_id, "goal-1");
        assert_eq!(constitution.created_by, "ta-supervisor");
        assert_eq!(constitution.access.len(), 2);
        assert_eq!(constitution.enforcement, EnforcementMode::Warning);
    }
}
