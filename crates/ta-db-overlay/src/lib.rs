//! ta-db-overlay — DraftOverlay for DB proxy plugins.
//!
//! Provides read-your-writes consistency for database mutations during a goal.
//! All writes go through the overlay; reads check the overlay before hitting
//! the real database. The overlay is persisted to JSONL for human review.

pub mod entry;
pub mod error;
pub mod overlay;

pub use entry::{BlobRef, OverlayEntry, OverlayEntryKind};
pub use error::OverlayError;
pub use overlay::DraftOverlay;
