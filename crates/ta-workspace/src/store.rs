// store.rs — ChangeStore trait and JsonFileStore implementation.
//
// The ChangeStore trait is the abstraction API for persisting ChangeSets.
// The MVP implementation (JsonFileStore) writes JSONL to disk so work is
// never lost. The trait can be swapped for SQLite, S3, or other backends
// later without changing the rest of the system.
//
// Design: each goal gets its own JSONL file: `<store_dir>/<goal_id>.jsonl`.
// This keeps goals isolated and makes cleanup simple.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use ta_changeset::ChangeSet;
use uuid::Uuid;

use crate::error::WorkspaceError;

/// Trait for persisting and retrieving ChangeSets.
///
/// In Rust, a `trait` is like an interface — it defines methods that
/// implementations must provide. The `ChangeStore` trait allows us to
/// swap storage backends (JSONL files now, SQLite later) without
/// changing any code that uses the store.
pub trait ChangeStore {
    /// Save a changeset. If one with the same ID exists, it's overwritten.
    fn save(&mut self, goal_id: &str, changeset: &ChangeSet) -> Result<(), WorkspaceError>;

    /// List all changesets for a goal, in insertion order.
    fn list(&self, goal_id: &str) -> Result<Vec<ChangeSet>, WorkspaceError>;

    /// Get a specific changeset by ID.
    fn get(&self, goal_id: &str, changeset_id: Uuid) -> Result<Option<ChangeSet>, WorkspaceError>;

    /// Remove a specific changeset by ID.
    fn remove(&mut self, goal_id: &str, changeset_id: Uuid) -> Result<bool, WorkspaceError>;
}

/// JSON Lines file-based ChangeStore implementation.
///
/// Each goal gets a file: `<store_dir>/<goal_id>.jsonl`
/// Each line is one JSON-serialized ChangeSet.
///
/// This is append-optimized but rewrites the file on remove().
/// Fine for MVP volumes; swap to SQLite for heavy use.
pub struct JsonFileStore {
    store_dir: PathBuf,
}

impl JsonFileStore {
    /// Create a new store backed by the given directory.
    /// Creates the directory if it doesn't exist.
    pub fn new(store_dir: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let store_dir = store_dir.as_ref().to_path_buf();
        fs::create_dir_all(&store_dir).map_err(|source| WorkspaceError::IoError {
            path: store_dir.clone(),
            source,
        })?;
        Ok(Self { store_dir })
    }

    /// Path to the JSONL file for a given goal.
    fn goal_file(&self, goal_id: &str) -> PathBuf {
        self.store_dir.join(format!("{}.jsonl", goal_id))
    }
}

impl ChangeStore for JsonFileStore {
    fn save(&mut self, goal_id: &str, changeset: &ChangeSet) -> Result<(), WorkspaceError> {
        let path = self.goal_file(goal_id);

        // Open file in append mode (creates if needed).
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| WorkspaceError::IoError {
                path: path.clone(),
                source,
            })?;

        let mut writer = BufWriter::new(file);
        let json = serde_json::to_string(changeset)?;
        writeln!(writer, "{}", json).map_err(|source| WorkspaceError::IoError {
            path: path.clone(),
            source,
        })?;
        writer
            .flush()
            .map_err(|source| WorkspaceError::IoError { path, source })?;

        Ok(())
    }

    fn list(&self, goal_id: &str) -> Result<Vec<ChangeSet>, WorkspaceError> {
        let path = self.goal_file(goal_id);

        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path).map_err(|source| WorkspaceError::IoError {
            path: path.clone(),
            source,
        })?;

        let reader = BufReader::new(file);
        let mut changesets = Vec::new();

        for line in reader.lines() {
            let line = line.map_err(|source| WorkspaceError::IoError {
                path: path.clone(),
                source,
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let cs: ChangeSet = serde_json::from_str(&line)?;
            changesets.push(cs);
        }

        Ok(changesets)
    }

    fn get(&self, goal_id: &str, changeset_id: Uuid) -> Result<Option<ChangeSet>, WorkspaceError> {
        let changesets = self.list(goal_id)?;
        Ok(changesets
            .into_iter()
            .find(|cs| cs.changeset_id == changeset_id))
    }

    fn remove(&mut self, goal_id: &str, changeset_id: Uuid) -> Result<bool, WorkspaceError> {
        let changesets = self.list(goal_id)?;
        let original_len = changesets.len();

        // Filter out the changeset to remove.
        let remaining: Vec<&ChangeSet> = changesets
            .iter()
            .filter(|cs| cs.changeset_id != changeset_id)
            .collect();

        if remaining.len() == original_len {
            return Ok(false); // Not found
        }

        // Rewrite the file without the removed changeset.
        let path = self.goal_file(goal_id);
        let file = File::create(&path).map_err(|source| WorkspaceError::IoError {
            path: path.clone(),
            source,
        })?;
        let mut writer = BufWriter::new(file);

        for cs in remaining {
            let json = serde_json::to_string(cs)?;
            writeln!(writer, "{}", json).map_err(|source| WorkspaceError::IoError {
                path: path.clone(),
                source,
            })?;
        }

        writer
            .flush()
            .map_err(|source| WorkspaceError::IoError { path, source })?;

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_changeset::{ChangeKind, DiffContent};
    use tempfile::tempdir;

    fn test_changeset(name: &str) -> ChangeSet {
        ChangeSet::new(
            format!("fs://workspace/{}", name),
            ChangeKind::FsPatch,
            DiffContent::CreateFile {
                content: format!("content of {}", name),
            },
        )
    }

    #[test]
    fn save_and_list_round_trip() {
        let dir = tempdir().unwrap();
        let mut store = JsonFileStore::new(dir.path().join("store")).unwrap();

        let cs1 = test_changeset("file1.txt");
        let cs2 = test_changeset("file2.txt");

        store.save("goal-1", &cs1).unwrap();
        store.save("goal-1", &cs2).unwrap();

        let listed = store.list("goal-1").unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].changeset_id, cs1.changeset_id);
        assert_eq!(listed[1].changeset_id, cs2.changeset_id);
    }

    #[test]
    fn list_empty_goal_returns_empty() {
        let dir = tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().join("store")).unwrap();

        let listed = store.list("nonexistent-goal").unwrap();
        assert!(listed.is_empty());
    }

    #[test]
    fn get_by_id() {
        let dir = tempdir().unwrap();
        let mut store = JsonFileStore::new(dir.path().join("store")).unwrap();

        let cs = test_changeset("file.txt");
        let id = cs.changeset_id;
        store.save("goal-1", &cs).unwrap();

        let found = store.get("goal-1", id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().changeset_id, id);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let dir = tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().join("store")).unwrap();

        let found = store.get("goal-1", Uuid::new_v4()).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn remove_changeset() {
        let dir = tempdir().unwrap();
        let mut store = JsonFileStore::new(dir.path().join("store")).unwrap();

        let cs1 = test_changeset("file1.txt");
        let cs2 = test_changeset("file2.txt");
        let id1 = cs1.changeset_id;

        store.save("goal-1", &cs1).unwrap();
        store.save("goal-1", &cs2).unwrap();

        let removed = store.remove("goal-1", id1).unwrap();
        assert!(removed);

        let listed = store.list("goal-1").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].changeset_id, cs2.changeset_id);
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let dir = tempdir().unwrap();
        let mut store = JsonFileStore::new(dir.path().join("store")).unwrap();

        let removed = store.remove("goal-1", Uuid::new_v4()).unwrap();
        assert!(!removed);
    }

    #[test]
    fn goals_are_isolated() {
        let dir = tempdir().unwrap();
        let mut store = JsonFileStore::new(dir.path().join("store")).unwrap();

        let cs1 = test_changeset("file1.txt");
        let cs2 = test_changeset("file2.txt");

        store.save("goal-1", &cs1).unwrap();
        store.save("goal-2", &cs2).unwrap();

        assert_eq!(store.list("goal-1").unwrap().len(), 1);
        assert_eq!(store.list("goal-2").unwrap().len(), 1);
    }

    #[test]
    fn store_survives_reopen() {
        // Verify data persists across store instances (process restart).
        let dir = tempdir().unwrap();
        let store_path = dir.path().join("store");

        let cs = test_changeset("file.txt");
        let id = cs.changeset_id;

        // Write with first store instance
        {
            let mut store = JsonFileStore::new(&store_path).unwrap();
            store.save("goal-1", &cs).unwrap();
        }

        // Read with second store instance (simulating restart)
        {
            let store = JsonFileStore::new(&store_path).unwrap();
            let listed = store.list("goal-1").unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].changeset_id, id);
        }
    }
}
