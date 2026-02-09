// log.rs — Append-only JSONL audit log.
//
// The audit log is stored as a JSONL (JSON Lines) file: one JSON object per
// line. This format is simple, append-friendly, and easy to parse with
// standard tools (jq, grep, etc.).
//
// Each event is linked to the previous one via `previous_hash`, forming a
// hash chain. This means any tampering (inserting, deleting, or modifying
// events) can be detected by verifying the chain.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::error::AuditError;
use crate::event::AuditEvent;
use crate::hasher;

/// An append-only audit log backed by a JSONL file.
///
/// In Rust, `BufWriter` wraps a `File` and batches writes for performance.
/// We flush after each event to ensure durability.
pub struct AuditLog {
    writer: BufWriter<File>,
    path: PathBuf,
    /// Hash of the last event written — used to set `previous_hash` on the next event.
    last_hash: Option<String>,
}

impl AuditLog {
    /// Open (or create) an audit log at the given path.
    ///
    /// If the file already exists, it reads the last event to recover the
    /// hash chain state so new events link correctly.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AuditError> {
        let path = path.as_ref().to_path_buf();

        // Recover the last hash from any existing log content.
        let last_hash = if path.exists() {
            Self::read_last_hash(&path)?
        } else {
            None
        };

        // Open in append mode — this ensures we never overwrite existing data.
        // `create(true)` creates the file if it doesn't exist.
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| AuditError::OpenFailed {
                path: path.clone(),
                source,
            })?;

        Ok(Self {
            writer: BufWriter::new(file),
            path,
            last_hash,
        })
    }

    /// Append an event to the log.
    ///
    /// Automatically sets the `previous_hash` field to chain this event
    /// to the last one. Flushes to disk after writing.
    pub fn append(&mut self, event: &mut AuditEvent) -> Result<(), AuditError> {
        // Link this event to the previous one.
        event.previous_hash = self.last_hash.clone();

        // Serialize to a single JSON line (no pretty-printing).
        let json = serde_json::to_string(event)?;

        // Compute and store the hash of this event for the next link.
        self.last_hash = Some(hasher::hash_str(&json));

        // Write the JSON line followed by a newline.
        writeln!(self.writer, "{}", json)?;

        // Flush to ensure durability — data is written to the OS.
        self.writer.flush()?;

        Ok(())
    }

    /// Read all events from a log file.
    ///
    /// Returns them in order (oldest first). Skips blank lines gracefully.
    pub fn read_all(path: impl AsRef<Path>) -> Result<Vec<AuditEvent>, AuditError> {
        let file = File::open(path.as_ref()).map_err(|source| AuditError::OpenFailed {
            path: path.as_ref().to_path_buf(),
            source,
        })?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let event: AuditEvent = serde_json::from_str(&line)?;
            events.push(event);
        }

        Ok(events)
    }

    /// Verify the integrity of a log file's hash chain.
    ///
    /// Reads all events and checks that each event's `previous_hash` matches
    /// the hash of the preceding event's JSON. Returns `Ok(true)` if valid,
    /// or an `IntegrityViolation` error if tampered.
    pub fn verify_chain(path: impl AsRef<Path>) -> Result<bool, AuditError> {
        let file = File::open(path.as_ref()).map_err(|source| AuditError::OpenFailed {
            path: path.as_ref().to_path_buf(),
            source,
        })?;
        let reader = BufReader::new(file);
        let mut previous_hash: Option<String> = None;

        for (line_num, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            // Parse the event to check its previous_hash field.
            let event: AuditEvent = serde_json::from_str(&line)?;

            // Verify the chain link.
            if event.previous_hash != previous_hash {
                return Err(AuditError::IntegrityViolation {
                    line: line_num + 1,
                    expected: previous_hash.unwrap_or_else(|| "None".to_string()),
                    actual: event.previous_hash.unwrap_or_else(|| "None".to_string()),
                });
            }

            // Compute the hash of this line for the next iteration.
            // Important: we hash the raw JSON line, not the re-serialized event,
            // because re-serialization might change field order.
            previous_hash = Some(hasher::hash_str(&line));
        }

        Ok(true)
    }

    /// Return the path to the log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the hash of the last event in an existing log file.
    fn read_last_hash(path: &Path) -> Result<Option<String>, AuditError> {
        let file = File::open(path).map_err(|source| AuditError::OpenFailed {
            path: path.to_path_buf(),
            source,
        })?;
        let reader = BufReader::new(file);
        let mut last_line: Option<String> = None;

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                last_line = Some(line);
            }
        }

        Ok(last_line.map(|line| hasher::hash_str(&line)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::AuditAction;
    use tempfile::tempdir;

    #[test]
    fn append_and_read_round_trip() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");

        // Write two events
        {
            let mut log = AuditLog::open(&log_path).unwrap();
            let mut e1 = AuditEvent::new("agent-1", AuditAction::ToolCall)
                .with_target("fs://workspace/file.txt");
            let mut e2 = AuditEvent::new("agent-1", AuditAction::PolicyDecision);
            log.append(&mut e1).unwrap();
            log.append(&mut e2).unwrap();
        }

        // Read them back
        let events = AuditLog::read_all(&log_path).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].action, AuditAction::ToolCall);
        assert_eq!(events[1].action, AuditAction::PolicyDecision);
    }

    #[test]
    fn hash_chain_is_valid() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");

        {
            let mut log = AuditLog::open(&log_path).unwrap();
            for i in 0..5 {
                let mut event = AuditEvent::new(format!("agent-{}", i), AuditAction::ToolCall);
                log.append(&mut event).unwrap();
            }
        }

        // Chain verification should succeed
        assert!(AuditLog::verify_chain(&log_path).unwrap());
    }

    #[test]
    fn first_event_has_no_previous_hash() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");

        {
            let mut log = AuditLog::open(&log_path).unwrap();
            let mut event = AuditEvent::new("agent-1", AuditAction::ToolCall);
            log.append(&mut event).unwrap();
        }

        let events = AuditLog::read_all(&log_path).unwrap();
        assert!(events[0].previous_hash.is_none());
    }

    #[test]
    fn second_event_links_to_first() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");

        {
            let mut log = AuditLog::open(&log_path).unwrap();
            let mut e1 = AuditEvent::new("agent-1", AuditAction::ToolCall);
            let mut e2 = AuditEvent::new("agent-1", AuditAction::Approval);
            log.append(&mut e1).unwrap();
            log.append(&mut e2).unwrap();
        }

        let events = AuditLog::read_all(&log_path).unwrap();
        // Second event should have a previous_hash that is not None.
        assert!(events[1].previous_hash.is_some());
    }

    #[test]
    fn reopen_log_continues_chain() {
        // Verify that closing and reopening the log maintains the hash chain.
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");

        // Write first event
        {
            let mut log = AuditLog::open(&log_path).unwrap();
            let mut event = AuditEvent::new("agent-1", AuditAction::ToolCall);
            log.append(&mut event).unwrap();
        }

        // Reopen and write second event
        {
            let mut log = AuditLog::open(&log_path).unwrap();
            let mut event = AuditEvent::new("agent-1", AuditAction::Approval);
            log.append(&mut event).unwrap();
        }

        // Chain should still be valid
        assert!(AuditLog::verify_chain(&log_path).unwrap());
        assert_eq!(AuditLog::read_all(&log_path).unwrap().len(), 2);
    }
}
