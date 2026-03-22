//! ta-db-proxy — DbProxyPlugin trait for TA database proxy governance.
//!
//! Plugins intercept agent database connections, enforce policies, and capture
//! mutations through DraftOverlay for human review.

pub mod classification;
pub mod error;
pub mod plugin;

pub use classification::{MutationKind, QueryClass};
pub use error::ProxyError;
pub use plugin::{DbProxyPlugin, ProxyConfig, ProxyHandle};
