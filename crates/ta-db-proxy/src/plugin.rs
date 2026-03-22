use std::net::SocketAddr;
use std::path::Path;

use crate::classification::QueryClass;
use crate::error::Result;

/// Configuration for starting a database proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// The local address the proxy will listen on (e.g., `127.0.0.1:15432`).
    pub listen_addr: SocketAddr,
    /// The real database connection string (forwarded after policy checks).
    pub upstream_dsn: String,
    /// Path to the staging directory (for DraftOverlay storage).
    pub staging_dir: std::path::PathBuf,
}

/// Handle to a running proxy instance. Dropping stops the proxy.
pub trait ProxyHandle: Send {
    /// The address the proxy is listening on.
    fn listen_addr(&self) -> SocketAddr;
    /// Stop the proxy gracefully.
    fn stop(&mut self);
}

/// Database proxy plugin trait.
///
/// Each database type (SQLite, Postgres, MongoDB, etc.) provides its own
/// implementation. TA calls `start()` before the agent runs and `stop()` after.
///
/// The plugin intercepts all DB operations:
/// - READs: checked against DraftOverlay first (read-your-writes); if not in
///   overlay, forwarded to real DB.
/// - WRITEs: captured in DraftOverlay, not forwarded to real DB during the draft.
/// - DDL: captured in DraftOverlay as DDLMutation, flagged for reviewer approval.
pub trait DbProxyPlugin: Send + Sync {
    /// Human-readable name of this plugin (e.g., "sqlite", "postgres", "mongodb").
    fn name(&self) -> &str;

    /// Wire protocol identifier (e.g., "sqlite-vfs", "postgres", "mongodb").
    fn wire_protocol(&self) -> &str;

    /// Start the proxy and return a handle. The proxy listens on `config.listen_addr`
    /// and connects to `config.upstream_dsn`.
    fn start(&self, config: ProxyConfig) -> Result<Box<dyn ProxyHandle>>;

    /// Classify a raw query string into READ/WRITE/DDL/ADMIN/UNKNOWN.
    /// Used for policy enforcement before forwarding.
    fn classify_query(&self, query: &str) -> QueryClass;

    /// Replay staged mutations against the real DB on `ta draft apply`.
    /// Called once per mutation in `DraftOverlay::list_mutations()` order.
    fn apply_mutation(
        &self,
        upstream_dsn: &str,
        uri: &str,
        before: Option<&serde_json::Value>,
        after: &serde_json::Value,
        staging_dir: &Path,
    ) -> Result<()>;
}
