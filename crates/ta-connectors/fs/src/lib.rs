//! # ta-connector-fs
//!
//! Filesystem connector for Trusted Autonomy.
//!
//! Bridges MCP-style tool operations (read, write_patch, diff) to the
//! staging workspace and changeset model. All writes go to a staging
//! directory; approved changes are applied to the real target via `apply()`.
//!
//! ## Flow
//!
//! 1. Agent calls [`FsConnector::write_patch`] → file staged, ChangeSet created
//! 2. Agent calls [`FsConnector::build_pr_package`] → bundles all changes
//! 3. Human reviews and approves the PR package
//! 4. Agent calls [`FsConnector::apply`] → copies staged files to real filesystem

pub mod connector;
pub mod error;

pub use connector::FsConnector;
pub use error::FsConnectorError;
