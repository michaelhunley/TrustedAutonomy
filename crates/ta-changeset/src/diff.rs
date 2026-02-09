// diff.rs — Diff content representations.
//
// A DiffContent describes what changed. It can be a text diff, a new file,
// a deleted file, or a summary for binary files (images, PDFs, etc.).
//
// This is the "what" — the ChangeSet wraps it with the "where" and "why".

use serde::{Deserialize, Serialize};

/// The actual content of a change.
///
/// Rust enums can carry different data per variant — this is called a
/// "tagged union" or "sum type". In JSON, serde uses a "type" tag to
/// distinguish variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DiffContent {
    /// A standard unified diff (like `git diff` output).
    UnifiedDiff {
        /// The diff text in unified format.
        content: String,
    },

    /// A brand new file is being created.
    CreateFile {
        /// The full file content as UTF-8 text.
        /// For binary files, use BinarySummary instead.
        content: String,
    },

    /// A file is being deleted entirely.
    DeleteFile,

    /// Summary for a binary file (no text diff possible).
    BinarySummary {
        /// MIME type (e.g., "image/png", "application/pdf").
        mime_type: String,
        /// File size in bytes.
        size_bytes: u64,
        /// SHA-256 hash of the binary content.
        hash: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_diff_serialization_round_trip() {
        let diff = DiffContent::UnifiedDiff {
            content: "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new".to_string(),
        };
        let json = serde_json::to_string(&diff).unwrap();
        let restored: DiffContent = serde_json::from_str(&json).unwrap();
        assert_eq!(diff, restored);
    }

    #[test]
    fn create_file_serialization() {
        let diff = DiffContent::CreateFile {
            content: "fn main() {}".to_string(),
        };
        let json = serde_json::to_string(&diff).unwrap();
        assert!(json.contains("\"create_file\""));
    }

    #[test]
    fn binary_summary_serialization() {
        let diff = DiffContent::BinarySummary {
            mime_type: "image/png".to_string(),
            size_bytes: 1024,
            hash: "abc123".to_string(),
        };
        let json = serde_json::to_string(&diff).unwrap();
        let restored: DiffContent = serde_json::from_str(&json).unwrap();
        assert_eq!(diff, restored);
    }
}
