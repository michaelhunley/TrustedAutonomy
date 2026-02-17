//! Workflow configuration structures

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level workflow configuration from .ta/workflow.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowConfig {
    /// Submit adapter configuration
    pub submit: SubmitConfig,

    /// Diff viewing configuration
    #[serde(default)]
    pub diff: DiffConfig,

    /// Display / output configuration
    #[serde(default)]
    pub display: DisplayConfig,
}

/// Submit adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitConfig {
    /// Adapter type: "git", "none", or future adapters
    #[serde(default = "default_adapter")]
    pub adapter: String,

    /// Auto-commit on `ta pr apply` (only active when .ta/workflow.toml exists)
    #[serde(default)]
    pub auto_commit: bool,

    /// Auto-push after commit
    #[serde(default)]
    pub auto_push: bool,

    /// Auto-create review (PR) after push
    #[serde(default)]
    pub auto_review: bool,

    /// Git-specific configuration
    #[serde(default)]
    pub git: GitConfig,
}

impl Default for SubmitConfig {
    fn default() -> Self {
        Self {
            adapter: default_adapter(),
            auto_commit: false,
            auto_push: false,
            auto_review: false,
            git: GitConfig::default(),
        }
    }
}

/// Git adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    /// Branch naming prefix (e.g., "ta/", "feature/")
    #[serde(default = "default_branch_prefix")]
    pub branch_prefix: String,

    /// Target branch for PRs (e.g., "main", "develop")
    #[serde(default = "default_target_branch")]
    pub target_branch: String,

    /// Merge strategy: "squash", "merge", "rebase"
    #[serde(default = "default_merge_strategy")]
    pub merge_strategy: String,

    /// Path to PR body template (optional)
    pub pr_template: Option<PathBuf>,

    /// Git remote name
    #[serde(default = "default_remote")]
    pub remote: String,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            branch_prefix: default_branch_prefix(),
            target_branch: default_target_branch(),
            merge_strategy: default_merge_strategy(),
            pr_template: None,
            remote: default_remote(),
        }
    }
}

// Serde default functions
fn default_adapter() -> String {
    "none".to_string()
}

fn default_branch_prefix() -> String {
    "ta/".to_string()
}

fn default_target_branch() -> String {
    "main".to_string()
}

fn default_merge_strategy() -> String {
    "squash".to_string()
}

fn default_remote() -> String {
    "origin".to_string()
}

/// Diff viewing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffConfig {
    /// Open files in external handlers by default when using `ta pr view --file`
    #[serde(default = "default_open_external")]
    pub open_external: bool,

    /// Optional path override for diff-handlers.toml (defaults to .ta/diff-handlers.toml)
    pub handlers_file: Option<PathBuf>,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            open_external: default_open_external(),
            handlers_file: None,
        }
    }
}

fn default_open_external() -> bool {
    true
}

/// Display / output configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Enable ANSI color output in terminal adapter. Default: false.
    /// Override per-command with `--color`.
    #[serde(default)]
    pub color: bool,
}

impl WorkflowConfig {
    /// Load workflow config from .ta/workflow.toml
    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Try to load config, returning default if file doesn't exist
    pub fn load_or_default(path: &std::path::Path) -> Self {
        Self::load(path).unwrap_or_default()
    }
}
