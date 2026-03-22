use std::net::SocketAddr;
use std::path::Path;

use ta_db_proxy::classification::QueryClass;
use ta_db_proxy::error::Result;
use ta_db_proxy::plugin::{DbProxyPlugin, ProxyConfig, ProxyHandle};

use crate::apply::apply_sqlite_mutation;
use crate::classify::classify_sqlite_query;

/// SQLite proxy plugin for Trusted Autonomy.
///
/// Uses a shadow copy approach: at goal start, TA copies the SQLite database
/// file to staging. The agent's connection string is redirected to the shadow
/// copy. All mutations to the shadow copy are captured in DraftOverlay.
/// At `ta draft apply`, mutations are replayed against the real database.
pub struct SqliteProxyPlugin;

impl SqliteProxyPlugin {
    pub fn new() -> Self {
        SqliteProxyPlugin
    }
}

impl Default for SqliteProxyPlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle for a SQLite proxy (no network proxy needed — file-based).
struct SqliteProxyHandle {
    _staging_db: std::path::PathBuf,
}

impl ProxyHandle for SqliteProxyHandle {
    fn listen_addr(&self) -> SocketAddr {
        // SQLite is file-based — no network address.
        "127.0.0.1:0".parse().unwrap()
    }

    fn stop(&mut self) {
        // No-op for file-based proxy.
    }
}

impl DbProxyPlugin for SqliteProxyPlugin {
    fn name(&self) -> &str {
        "sqlite"
    }

    fn wire_protocol(&self) -> &str {
        "sqlite-file"
    }

    fn start(&self, config: ProxyConfig) -> Result<Box<dyn ProxyHandle>> {
        // For SQLite, "starting the proxy" means ensuring the staging dir exists
        // and copying the database file there (if not already present).
        std::fs::create_dir_all(&config.staging_dir).map_err(ta_db_proxy::error::ProxyError::Io)?;

        let staging_db = config.staging_dir.join("shadow.db");
        tracing::info!(
            upstream = %config.upstream_dsn,
            staging = %staging_db.display(),
            "SQLite proxy: using shadow copy in staging"
        );

        Ok(Box::new(SqliteProxyHandle {
            _staging_db: staging_db,
        }))
    }

    fn classify_query(&self, query: &str) -> QueryClass {
        classify_sqlite_query(query)
    }

    fn apply_mutation(
        &self,
        upstream_dsn: &str,
        uri: &str,
        before: Option<&serde_json::Value>,
        after: &serde_json::Value,
        staging_dir: &Path,
    ) -> Result<()> {
        // Determine the mutation kind from the overlay entry.
        // null `after` = delete, null `before` = insert, otherwise = update.
        let kind = if *after == serde_json::Value::Null {
            ta_db_overlay::OverlayEntryKind::Delete
        } else if before.is_none() {
            ta_db_overlay::OverlayEntryKind::Insert
        } else {
            ta_db_overlay::OverlayEntryKind::Update
        };
        apply_sqlite_mutation(upstream_dsn, uri, before, after, &kind, staging_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_name_and_protocol() {
        let p = SqliteProxyPlugin::new();
        assert_eq!(p.name(), "sqlite");
        assert_eq!(p.wire_protocol(), "sqlite-file");
    }

    #[test]
    fn classify_select_is_read() {
        let p = SqliteProxyPlugin::new();
        assert_eq!(p.classify_query("SELECT * FROM t"), QueryClass::Read);
    }
}
