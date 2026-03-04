// fs_mediator.rs — FsMediator: ResourceMediator implementation for filesystem resources.
//
// Wraps the existing StagingWorkspace to implement the ResourceMediator trait.
// This is the first (and built-in) mediator — it handles `fs://` URIs.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use uuid::Uuid;

use crate::error::MediationError;
use crate::mediator::{
    ActionClassification, ApplyResult, MutationPreview, ProposedAction, ResourceMediator,
    StagedMutation,
};

/// Filesystem mediator — stages file writes in a workspace, applies on approval.
///
/// Delegates to `ta-workspace::StagingWorkspace` for the actual staging mechanics.
/// This adapter makes the existing staging system conform to the `ResourceMediator` trait.
pub struct FsMediator {
    /// Root of the staging workspace (where staged files live).
    staging_dir: PathBuf,
    /// Root of the source directory (the real filesystem).
    source_dir: PathBuf,
}

impl FsMediator {
    /// Create a new filesystem mediator.
    ///
    /// - `staging_dir`: where staged files are written before approval
    /// - `source_dir`: the real filesystem root (for generating diffs)
    pub fn new(staging_dir: PathBuf, source_dir: PathBuf) -> Self {
        Self {
            staging_dir,
            source_dir,
        }
    }

    /// Extract the relative path from a `fs://workspace/...` URI.
    fn relative_path(uri: &str) -> Result<&str, MediationError> {
        uri.strip_prefix("fs://workspace/")
            .ok_or_else(|| MediationError::InvalidUri {
                uri: uri.to_string(),
            })
    }
}

impl ResourceMediator for FsMediator {
    fn scheme(&self) -> &str {
        "fs"
    }

    fn stage(&self, action: ProposedAction) -> Result<StagedMutation, MediationError> {
        let rel_path = Self::relative_path(&action.target_uri)?;

        // Extract content from parameters.
        let content = action
            .parameters
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Write to staging directory.
        let staged_path = self.staging_dir.join(rel_path);
        if let Some(parent) = staged_path.parent() {
            fs::create_dir_all(parent).map_err(|source| MediationError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&staged_path, content.as_bytes()).map_err(|source| MediationError::Io {
            path: staged_path.clone(),
            source,
        })?;

        Ok(StagedMutation {
            mutation_id: Uuid::new_v4(),
            action,
            staged_at: Utc::now(),
            preview: None,
            staging_ref: staged_path.to_string_lossy().to_string(),
        })
    }

    fn preview(&self, staged: &StagedMutation) -> Result<MutationPreview, MediationError> {
        let rel_path = Self::relative_path(&staged.action.target_uri)?;
        let source_path = self.source_dir.join(rel_path);
        let staged_path = Path::new(&staged.staging_ref);

        let diff = if source_path.exists() && staged_path.exists() {
            let original = fs::read_to_string(&source_path).unwrap_or_default();
            let modified = fs::read_to_string(staged_path).unwrap_or_default();
            if original == modified {
                None
            } else {
                Some(format!(
                    "--- a/{}\n+++ b/{}\n(content differs)",
                    rel_path, rel_path
                ))
            }
        } else if staged_path.exists() {
            Some(format!("--- /dev/null\n+++ b/{}\n(new file)", rel_path))
        } else {
            None
        };

        let is_new_file = !source_path.exists();
        let summary = if is_new_file {
            format!("Create new file: {}", rel_path)
        } else {
            format!("Modify file: {}", rel_path)
        };

        Ok(MutationPreview {
            summary,
            diff,
            risk_flags: vec![],
            classification: self.classify(&staged.action),
        })
    }

    fn apply(&self, staged: &StagedMutation) -> Result<ApplyResult, MediationError> {
        let rel_path = Self::relative_path(&staged.action.target_uri)?;
        let staged_path = Path::new(&staged.staging_ref);
        let target_path = self.source_dir.join(rel_path);

        if !staged_path.exists() {
            return Err(MediationError::ApplyFailed {
                uri: staged.action.target_uri.clone(),
                reason: "staged file does not exist".to_string(),
            });
        }

        // Ensure parent directories exist in target.
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|source| MediationError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let content = fs::read(staged_path).map_err(|source| MediationError::Io {
            path: staged_path.to_path_buf(),
            source,
        })?;
        fs::write(&target_path, &content).map_err(|source| MediationError::Io {
            path: target_path,
            source,
        })?;

        Ok(ApplyResult {
            mutation_id: staged.mutation_id,
            success: true,
            message: format!("Applied {} to source", rel_path),
            applied_at: Utc::now(),
        })
    }

    fn rollback(&self, staged: &StagedMutation) -> Result<(), MediationError> {
        let staged_path = Path::new(&staged.staging_ref);
        if staged_path.exists() {
            fs::remove_file(staged_path).map_err(|source| MediationError::Io {
                path: staged_path.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }

    fn classify(&self, action: &ProposedAction) -> ActionClassification {
        match action.verb.as_str() {
            "read" | "list" | "diff" => ActionClassification::ReadOnly,
            "write" | "write_patch" | "create" | "modify" => ActionClassification::StateChanging,
            "delete" => ActionClassification::StateChanging,
            "apply" | "commit" => ActionClassification::StateChanging,
            _ => ActionClassification::StateChanging,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (FsMediator, PathBuf, PathBuf) {
        let staging = tempdir().unwrap();
        let source = tempdir().unwrap();
        let staging_path = staging.keep();
        let source_path = source.keep();
        let mediator = FsMediator::new(staging_path.clone(), source_path.clone());
        (mediator, staging_path, source_path)
    }

    #[test]
    fn stage_creates_file_in_staging() {
        let (mediator, staging_path, _) = setup();

        let action = ProposedAction::new("fs", "write", "fs://workspace/hello.txt")
            .with_parameters(serde_json::json!({"content": "Hello, world!"}));

        let staged = mediator.stage(action).unwrap();
        assert!(Path::new(&staged.staging_ref).exists());

        let content = fs::read_to_string(staging_path.join("hello.txt")).unwrap();
        assert_eq!(content, "Hello, world!");
    }

    #[test]
    fn stage_creates_nested_directories() {
        let (mediator, staging_path, _) = setup();

        let action = ProposedAction::new("fs", "write", "fs://workspace/src/deep/file.rs")
            .with_parameters(serde_json::json!({"content": "fn main() {}"}));

        mediator.stage(action).unwrap();
        assert!(staging_path.join("src/deep/file.rs").exists());
    }

    #[test]
    fn preview_new_file() {
        let (mediator, _, _) = setup();

        let action = ProposedAction::new("fs", "write", "fs://workspace/new.txt")
            .with_parameters(serde_json::json!({"content": "new content"}));

        let staged = mediator.stage(action).unwrap();
        let preview = mediator.preview(&staged).unwrap();

        assert!(preview.summary.contains("Create new file"));
        assert!(preview.diff.is_some());
        assert!(preview.diff.unwrap().contains("/dev/null"));
    }

    #[test]
    fn preview_modified_file() {
        let (mediator, _, source_path) = setup();

        // Create an existing file in source.
        fs::write(source_path.join("existing.txt"), "original").unwrap();

        let action = ProposedAction::new("fs", "write", "fs://workspace/existing.txt")
            .with_parameters(serde_json::json!({"content": "modified"}));

        let staged = mediator.stage(action).unwrap();
        let preview = mediator.preview(&staged).unwrap();

        assert!(preview.summary.contains("Modify file"));
        assert!(preview.diff.is_some());
    }

    #[test]
    fn apply_copies_to_source() {
        let (mediator, _, source_path) = setup();

        let action = ProposedAction::new("fs", "write", "fs://workspace/applied.txt")
            .with_parameters(serde_json::json!({"content": "applied content"}));

        let staged = mediator.stage(action).unwrap();
        let result = mediator.apply(&staged).unwrap();

        assert!(result.success);
        let content = fs::read_to_string(source_path.join("applied.txt")).unwrap();
        assert_eq!(content, "applied content");
    }

    #[test]
    fn rollback_removes_staged_file() {
        let (mediator, _, _) = setup();

        let action = ProposedAction::new("fs", "write", "fs://workspace/rollback.txt")
            .with_parameters(serde_json::json!({"content": "temporary"}));

        let staged = mediator.stage(action).unwrap();
        assert!(Path::new(&staged.staging_ref).exists());

        mediator.rollback(&staged).unwrap();
        assert!(!Path::new(&staged.staging_ref).exists());
    }

    #[test]
    fn classify_read_actions() {
        let (mediator, _, _) = setup();

        let read = ProposedAction::new("fs", "read", "fs://workspace/file.txt");
        assert_eq!(mediator.classify(&read), ActionClassification::ReadOnly);

        let list = ProposedAction::new("fs", "list", "fs://workspace/");
        assert_eq!(mediator.classify(&list), ActionClassification::ReadOnly);
    }

    #[test]
    fn classify_write_actions() {
        let (mediator, _, _) = setup();

        let write = ProposedAction::new("fs", "write", "fs://workspace/file.txt");
        assert_eq!(
            mediator.classify(&write),
            ActionClassification::StateChanging
        );

        let delete = ProposedAction::new("fs", "delete", "fs://workspace/file.txt");
        assert_eq!(
            mediator.classify(&delete),
            ActionClassification::StateChanging
        );
    }

    #[test]
    fn invalid_uri_rejected() {
        let (mediator, _, _) = setup();

        let action = ProposedAction::new("fs", "write", "invalid://no/prefix")
            .with_parameters(serde_json::json!({"content": "data"}));

        let result = mediator.stage(action);
        assert!(result.is_err());
    }
}
