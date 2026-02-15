//! # ta-changeset
//!
//! The universal "staged mutation" data model for Trusted Autonomy.
//!
//! A [`ChangeSet`] represents any pending change — a file patch, email draft,
//! DB mutation, or social media post. All changes are collected (staged) by
//! default and bundled into a [`PRPackage`] for human review.
//!
//! The data model aligns with `schema/pr_package.schema.json`.

pub mod changeset;
pub mod diff;
pub mod diff_handlers;
pub mod error;
pub mod pr_package;
pub mod uri_pattern;

pub use changeset::{ChangeKind, ChangeSet, CommitIntent};
pub use diff::DiffContent;
pub use diff_handlers::{DiffHandlerError, DiffHandlersConfig, HandlerRule};
pub use error::ChangeSetError;
pub use pr_package::{PRPackage, PRStatus};
pub use uri_pattern::{filter_uris, matches_uri};
