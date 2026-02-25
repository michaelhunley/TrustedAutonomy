//! # ta-changeset
//!
//! The universal "staged mutation" data model for Trusted Autonomy.
//!
//! A [`ChangeSet`] represents any pending change — a file patch, email draft,
//! DB mutation, or social media post. All changes are collected (staged) by
//! default and bundled into a [`DraftPackage`] for human review.
//!
//! The data model aligns with `schema/draft_package.schema.json`.

pub mod changeset;
pub mod diff;
pub mod diff_handlers;
pub mod draft_package;
pub mod error;
pub mod explanation;
pub mod interactive_session_store;
pub mod output_adapters;
pub mod pr_package;
pub mod review_session;
pub mod review_session_store;
pub mod session_channel;
pub mod supervisor;
pub mod uri_pattern;

pub use changeset::{ChangeKind, ChangeSet, CommitIntent};
pub use diff::DiffContent;
pub use diff_handlers::{DiffHandlerError, DiffHandlersConfig, HandlerRule};
pub use draft_package::{DraftPackage, DraftStatus, ExplanationTiers};
pub use error::ChangeSetError;
pub use explanation::ExplanationSidecar;
pub use interactive_session_store::InteractiveSessionStore;
pub use output_adapters::{DetailLevel, OutputAdapter, OutputFormat, RenderContext};
pub use review_session::{
    ArtifactReview, Comment, CommentThread, DispositionCounts, ReviewSession, ReviewState,
    SessionNote,
};
pub use review_session_store::ReviewSessionStore;
pub use session_channel::{
    HumanInput, InteractiveConfig, InteractiveSession, InteractiveSessionState, OutputStream,
    SessionChannel, SessionChannelError, SessionEvent, SessionMessage,
};
pub use supervisor::{
    DependencyGraph, SupervisorAgent, ValidationError, ValidationResult, ValidationWarning,
};
pub use uri_pattern::{filter_uris, matches_uri};

// Backwards compatibility: export old names as aliases
pub use draft_package::DraftPackage as PRPackage;
pub use draft_package::DraftStatus as PRStatus;
