// changeset.rs — The universal staged mutation type.
//
// A ChangeSet represents any pending change in the system — a file patch,
// email draft, DB mutation, or social post. All changes are staged (collected)
// by default and bundled into a PR package for review.
//
// The key insight: by representing ALL mutations uniformly, we can review
// filesystem changes, email drafts, and DB writes in a single PR package.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::diff::DiffContent;

/// What kind of mutation this changeset represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// A filesystem file patch (create, modify, delete).
    FsPatch,
    /// A database mutation.
    DbPatch,
    /// An email draft.
    EmailDraft,
    /// A social media post draft.
    SocialDraft,
    /// Any other kind of mutation.
    Other(String),
}

/// What the agent intends to do when the change is approved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitIntent {
    /// No specific commit intent — just staging for review.
    None,
    /// Request to apply filesystem changes.
    RequestCommit,
    /// Request to send an email.
    RequestSend,
    /// Request to publish a social media post.
    RequestPost,
}

/// A single staged mutation — the fundamental unit of the review system.
///
/// Every change an agent makes flows through this type, whether it's a
/// file edit, email draft, or database write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSet {
    /// Unique identifier for this changeset.
    pub changeset_id: Uuid,

    /// The resource being changed (e.g., "fs://workspace/src/main.rs").
    pub target_uri: String,

    /// What kind of mutation this is.
    pub kind: ChangeKind,

    /// The actual change content (diff, new file, etc.).
    pub diff_content: DiffContent,

    /// Optional pointer to a rendered preview (e.g., a diff viewer URL).
    pub preview_ref: Option<String>,

    /// Risk flags identified by the system (e.g., "contains_secrets", "large_change").
    pub risk_flags: Vec<String>,

    /// What the agent intends to do with this change once approved.
    pub commit_intent: CommitIntent,

    /// When this changeset was created.
    pub created_at: DateTime<Utc>,

    /// SHA-256 hash of the diff content for integrity verification.
    pub content_hash: String,
}

impl ChangeSet {
    /// Create a new changeset with automatically computed content hash.
    ///
    /// The content hash is computed from the serialized diff_content,
    /// ensuring integrity can be verified later.
    pub fn new(target_uri: String, kind: ChangeKind, diff_content: DiffContent) -> Self {
        let content_hash = compute_content_hash(&diff_content);
        Self {
            changeset_id: Uuid::new_v4(),
            target_uri,
            kind,
            diff_content,
            preview_ref: None,
            risk_flags: Vec::new(),
            commit_intent: CommitIntent::None,
            created_at: Utc::now(),
            content_hash,
        }
    }

    /// Set the commit intent and return self (builder pattern).
    pub fn with_commit_intent(mut self, intent: CommitIntent) -> Self {
        self.commit_intent = intent;
        self
    }

    /// Add a risk flag and return self.
    pub fn with_risk_flag(mut self, flag: impl Into<String>) -> Self {
        self.risk_flags.push(flag.into());
        self
    }

    /// Verify the content hash matches the actual diff content.
    pub fn verify_hash(&self) -> bool {
        let expected = compute_content_hash(&self.diff_content);
        self.content_hash == expected
    }
}

/// Compute SHA-256 hash of serialized diff content.
fn compute_content_hash(diff: &DiffContent) -> String {
    let json = serde_json::to_string(diff).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changeset_creation_computes_hash() {
        let cs = ChangeSet::new(
            "fs://workspace/test.txt".to_string(),
            ChangeKind::FsPatch,
            DiffContent::CreateFile {
                content: "hello world".to_string(),
            },
        );
        assert!(!cs.content_hash.is_empty());
        assert_eq!(cs.content_hash.len(), 64); // SHA-256 hex length
    }

    #[test]
    fn changeset_hash_is_deterministic() {
        let diff = DiffContent::CreateFile {
            content: "hello".to_string(),
        };
        let cs1 = ChangeSet::new("uri".to_string(), ChangeKind::FsPatch, diff.clone());
        let cs2 = ChangeSet::new("uri".to_string(), ChangeKind::FsPatch, diff);
        assert_eq!(cs1.content_hash, cs2.content_hash);
    }

    #[test]
    fn changeset_hash_verification() {
        let cs = ChangeSet::new(
            "fs://workspace/test.txt".to_string(),
            ChangeKind::FsPatch,
            DiffContent::CreateFile {
                content: "hello".to_string(),
            },
        );
        assert!(cs.verify_hash());
    }

    #[test]
    fn changeset_serialization_round_trip() {
        let cs = ChangeSet::new(
            "fs://workspace/test.txt".to_string(),
            ChangeKind::FsPatch,
            DiffContent::CreateFile {
                content: "hello".to_string(),
            },
        )
        .with_commit_intent(CommitIntent::RequestCommit)
        .with_risk_flag("large_change");

        let json = serde_json::to_string(&cs).unwrap();
        let restored: ChangeSet = serde_json::from_str(&json).unwrap();

        assert_eq!(cs.changeset_id, restored.changeset_id);
        assert_eq!(cs.target_uri, restored.target_uri);
        assert_eq!(cs.content_hash, restored.content_hash);
        assert_eq!(cs.risk_flags, restored.risk_flags);
        assert_eq!(cs.commit_intent, restored.commit_intent);
    }

    #[test]
    fn change_kind_serializes_as_snake_case() {
        let json = serde_json::to_string(&ChangeKind::FsPatch).unwrap();
        assert_eq!(json, "\"fs_patch\"");

        let json = serde_json::to_string(&ChangeKind::EmailDraft).unwrap();
        assert_eq!(json, "\"email_draft\"");
    }
}
