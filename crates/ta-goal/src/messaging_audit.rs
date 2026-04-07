//! Audit record for TA-created email drafts.
//!
//! Every draft created via a `MessagingAdapter` is recorded as a
//! `DraftEmailRecord` and appended to `.ta/messaging-audit.jsonl`.
//! The log is append-only; no records are deleted or modified.
//!
//! ## Usage
//!
//! ```ignore
//! let record = DraftEmailRecord {
//!     draft_id: "gmail-draft-abc123".to_string(),
//!     provider: "gmail".to_string(),
//!     to: "bob@example.com".to_string(),
//!     subject: "Re: Hello".to_string(),
//!     created_at: chrono::Utc::now().to_rfc3339(),
//!     state: DraftEmailState::Drafted,
//!     goal_id: Some(goal.goal_run_id.to_string()),
//!     constitution_check_passed: Some(true),
//!     supervisor_score: Some(0.97),
//! };
//! MessagingAuditLog::append(&project_root, &record)?;
//! ```

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Record
// ---------------------------------------------------------------------------

/// Current lifecycle state of a TA-created email draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftEmailState {
    /// Draft exists in the provider's Drafts folder.
    Drafted,
    /// User sent the draft from their email client.
    Sent,
    /// User deleted the draft without sending.
    Discarded,
    /// State is unknown (provider did not report definitively).
    Unknown,
}

impl std::fmt::Display for DraftEmailState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DraftEmailState::Drafted => write!(f, "drafted"),
            DraftEmailState::Sent => write!(f, "sent"),
            DraftEmailState::Discarded => write!(f, "discarded"),
            DraftEmailState::Unknown => write!(f, "unknown"),
        }
    }
}

/// Audit record for a single TA-created email draft.
///
/// Persisted as a JSON line in `.ta/messaging-audit.jsonl`.
/// One record per draft; `state` is updated when `draft_status` is polled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftEmailRecord {
    /// Provider-specific draft identifier (e.g., "gmail-draft-abc123").
    pub draft_id: String,

    /// Messaging provider that created this draft (e.g., "gmail", "outlook").
    pub provider: String,

    /// Recipient address(es).
    pub to: String,

    /// Subject line.
    pub subject: String,

    /// ISO-8601 timestamp when the draft was created by TA.
    pub created_at: String,

    /// Current lifecycle state.
    pub state: DraftEmailState,

    /// Goal run that produced this draft (if created within a goal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,

    /// Whether the draft passed the constitution / user voice policy check.
    /// `None` if the check was not run (e.g., in a direct adapter call).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constitution_check_passed: Option<bool>,

    /// Supervisor confidence score [0.0, 1.0] assigned by the review step.
    /// `None` if no supervisory review was performed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supervisor_score: Option<f64>,
}

impl DraftEmailRecord {
    /// Update the state field in-place (does not persist; call `audit_log.update` separately).
    pub fn set_state(&mut self, state: DraftEmailState) {
        self.state = state;
    }
}

// ---------------------------------------------------------------------------
// Log
// ---------------------------------------------------------------------------

/// Append-only log of `DraftEmailRecord` entries stored at
/// `.ta/messaging-audit.jsonl`.
pub struct MessagingAuditLog {
    path: PathBuf,
}

impl MessagingAuditLog {
    /// Open the log at its default location under `project_root`.
    pub fn open(project_root: &Path) -> Self {
        Self {
            path: project_root.join(".ta").join("messaging-audit.jsonl"),
        }
    }

    /// Open the log at an explicit path.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Path to the log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a record to the log.
    ///
    /// Creates the file (and parent directories) if they do not exist.
    pub fn append(&self, record: &DraftEmailRecord) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", json)?;

        Ok(())
    }

    /// Read all records from the log. Returns an empty vec if the file does
    /// not exist. Malformed lines are skipped with a warning.
    pub fn read_all(&self) -> std::io::Result<Vec<DraftEmailRecord>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }

        let file = std::fs::File::open(&self.path)?;
        let reader = std::io::BufReader::new(file);
        let mut records = Vec::new();

        for (line_no, line) in reader.lines().enumerate() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<DraftEmailRecord>(trimmed) {
                Ok(r) => records.push(r),
                Err(e) => {
                    tracing::warn!(
                        line = line_no + 1,
                        path = %self.path.display(),
                        error = %e,
                        "Skipping malformed messaging audit record"
                    );
                }
            }
        }

        Ok(records)
    }

    /// Append a record directly from project root (convenience wrapper).
    pub fn append_to(project_root: &Path, record: &DraftEmailRecord) -> std::io::Result<()> {
        Self::open(project_root).append(record)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(draft_id: &str) -> DraftEmailRecord {
        DraftEmailRecord {
            draft_id: draft_id.to_string(),
            provider: "gmail".to_string(),
            to: "bob@example.com".to_string(),
            subject: "Re: Hello".to_string(),
            created_at: "2026-04-06T10:00:00Z".to_string(),
            state: DraftEmailState::Drafted,
            goal_id: Some("goal-abc123".to_string()),
            constitution_check_passed: Some(true),
            supervisor_score: Some(0.95),
        }
    }

    #[test]
    fn append_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let log = MessagingAuditLog::open(dir.path());

        let r1 = make_record("draft-001");
        let r2 = make_record("draft-002");

        log.append(&r1).unwrap();
        log.append(&r2).unwrap();

        let records = log.read_all().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].draft_id, "draft-001");
        assert_eq!(records[1].draft_id, "draft-002");
    }

    #[test]
    fn read_all_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let log = MessagingAuditLog::open(dir.path());
        let records = log.read_all().unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn append_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let log = MessagingAuditLog::at(dir.path().join("sub").join("nested").join("audit.jsonl"));
        let record = make_record("draft-abc");
        log.append(&record).unwrap();
        assert!(log.path().exists());
    }

    #[test]
    fn record_state_display() {
        assert_eq!(DraftEmailState::Drafted.to_string(), "drafted");
        assert_eq!(DraftEmailState::Sent.to_string(), "sent");
        assert_eq!(DraftEmailState::Discarded.to_string(), "discarded");
        assert_eq!(DraftEmailState::Unknown.to_string(), "unknown");
    }

    #[test]
    fn set_state_updates_in_place() {
        let mut record = make_record("draft-xyz");
        assert_eq!(record.state, DraftEmailState::Drafted);
        record.set_state(DraftEmailState::Sent);
        assert_eq!(record.state, DraftEmailState::Sent);
    }

    #[test]
    fn record_serialization_skips_none_fields() {
        let record = DraftEmailRecord {
            draft_id: "draft-minimal".to_string(),
            provider: "imap".to_string(),
            to: "bob@example.com".to_string(),
            subject: "Hello".to_string(),
            created_at: "2026-04-06T10:00:00Z".to_string(),
            state: DraftEmailState::Drafted,
            goal_id: None,
            constitution_check_passed: None,
            supervisor_score: None,
        };

        let json = serde_json::to_string(&record).unwrap();
        assert!(!json.contains("goal_id"));
        assert!(!json.contains("constitution_check_passed"));
        assert!(!json.contains("supervisor_score"));
    }

    #[test]
    fn record_roundtrip_with_all_fields() {
        let record = make_record("draft-full");
        let json = serde_json::to_string(&record).unwrap();
        let parsed: DraftEmailRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.draft_id, "draft-full");
        assert_eq!(parsed.provider, "gmail");
        assert_eq!(parsed.state, DraftEmailState::Drafted);
        assert_eq!(parsed.constitution_check_passed, Some(true));
        assert!((parsed.supervisor_score.unwrap() - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn append_to_convenience_wrapper() {
        let dir = tempfile::tempdir().unwrap();
        let record = make_record("draft-via-convenience");
        MessagingAuditLog::append_to(dir.path(), &record).unwrap();

        let log = MessagingAuditLog::open(dir.path());
        let records = log.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].draft_id, "draft-via-convenience");
    }

    #[test]
    fn skips_malformed_lines_continues_reading() {
        let dir = tempfile::tempdir().unwrap();
        let log = MessagingAuditLog::open(dir.path());

        // Append one good record, then a bad line, then another good record.
        let good = make_record("draft-good");
        log.append(&good).unwrap();
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(log.path())
                .unwrap();
            writeln!(f, "{{this is not valid json}}").unwrap();
        }
        log.append(&good).unwrap();

        let records = log.read_all().unwrap();
        // Two good records; the bad line is silently skipped.
        assert_eq!(records.len(), 2);
    }
}
