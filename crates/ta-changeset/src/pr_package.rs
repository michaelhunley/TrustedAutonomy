//! pr_package.rs — Backwards compatibility re-exports for PRPackage → DraftPackage rename.
//!
//! This module re-exports all types from draft_package to maintain backwards compatibility
//! during the terminology transition (v0.2.4).

pub use crate::draft_package::*;

// Explicit re-exports for clarity (these are already covered by the glob above)
pub use crate::draft_package::{DraftPackage as PRPackage, DraftStatus as PRStatus};
