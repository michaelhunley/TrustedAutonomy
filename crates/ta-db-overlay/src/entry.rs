use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single mutation entry in the overlay JSONL file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayEntry {
    /// Resource URI identifying the DB record. Format depends on the DB type:
    /// - SQLite:   `sqlite://<db_path>/<table>/<rowid>`
    /// - Postgres: `postgres://<host>/<db>/<schema>/<table>/<pk>`
    /// - MongoDB:  `mongodb://<host>/<db>/<collection>/<doc_id>`
    pub uri: String,
    /// Original value before any writes in this draft (captured on first write).
    /// `None` for INSERT operations (record didn't exist).
    pub before: Option<serde_json::Value>,
    /// Latest staged value. Applies accumulate: `before` stays fixed.
    pub after: serde_json::Value,
    /// When this entry was last updated.
    pub ts: DateTime<Utc>,
    /// Entry kind (data mutation, DDL, blob reference, etc.)
    pub kind: OverlayEntryKind,
}

/// The type of overlay entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OverlayEntryKind {
    /// Regular row/document update.
    Update,
    /// New record insertion.
    Insert,
    /// Record deletion.
    Delete,
    /// Schema change (DDL). Requires explicit reviewer approval.
    Ddl,
    /// Binary blob reference (content stored separately by SHA-256).
    Blob,
}

/// Reference to a binary blob stored in the blobs subdirectory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobRef {
    pub sha256: String,
    pub size_bytes: u64,
    pub field: String,
}
