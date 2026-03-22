//! ta-db-proxy-sqlite — SQLite proxy plugin for Trusted Autonomy.
//!
//! Implements DbProxyPlugin for SQLite databases. Uses a shadow copy approach:
//! TA keeps a copy of the SQLite file in staging. The agent uses this shadow copy
//! instead of the real DB. Mutations are captured and the shadow diff is used to
//! replay on the real DB at `ta draft apply` time.
//!
//! This is simpler than a wire protocol proxy since SQLite is file-based.

pub mod apply;
pub mod classify;
pub mod plugin;

pub use plugin::SqliteProxyPlugin;
