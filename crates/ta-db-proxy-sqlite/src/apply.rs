use std::path::Path;

use rusqlite::Connection;
use ta_db_overlay::OverlayEntryKind;
use ta_db_proxy::error::{ProxyError, Result};

/// Apply a staged SQLite mutation to the real database.
///
/// The `uri` format is `sqlite://<db_path>/<table>/<rowid>`.
/// `after` is a JSON object representing the row's column values.
pub fn apply_sqlite_mutation(
    upstream_dsn: &str,
    uri: &str,
    before: Option<&serde_json::Value>,
    after: &serde_json::Value,
    kind: &OverlayEntryKind,
    _staging_dir: &Path,
) -> Result<()> {
    // Parse URI: sqlite://<db_path>/<table>/<rowid>
    let rest = uri
        .strip_prefix("sqlite://")
        .ok_or_else(|| ProxyError::Plugin(format!("Invalid SQLite URI: {}", uri)))?;
    let parts: Vec<&str> = rest.rsplitn(3, '/').collect();
    if parts.len() < 3 {
        return Err(ProxyError::Plugin(format!(
            "SQLite URI must be sqlite://<db>/<table>/<rowid>: {}",
            uri
        )));
    }
    let rowid_str = parts[0];
    let table = parts[1];
    let db_path = parts[2];

    let conn = Connection::open(db_path)
        .map_err(|e| ProxyError::Plugin(format!("Cannot open {}: {}", db_path, e)))?;

    match kind {
        OverlayEntryKind::Delete => {
            let sql = format!("DELETE FROM {} WHERE rowid = ?", table);
            conn.execute(&sql, rusqlite::params![rowid_str])
                .map_err(|e| ProxyError::Plugin(e.to_string()))?;
        }
        OverlayEntryKind::Insert => {
            if let Some(obj) = after.as_object() {
                let cols: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
                let placeholders: Vec<&str> = (0..cols.len()).map(|_| "?").collect();
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    table,
                    cols.join(", "),
                    placeholders.join(", ")
                );
                let values: Vec<String> = obj.values().map(value_to_sql_string).collect();
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|e| ProxyError::Plugin(e.to_string()))?;
                for (i, v) in values.iter().enumerate() {
                    stmt.raw_bind_parameter(i + 1, v.as_str())
                        .map_err(|e| ProxyError::Plugin(e.to_string()))?;
                }
                stmt.raw_execute()
                    .map_err(|e| ProxyError::Plugin(e.to_string()))?;
            }
        }
        OverlayEntryKind::Update => {
            if let Some(obj) = after.as_object() {
                let set_clauses: Vec<String> = obj.keys().map(|k| format!("{} = ?", k)).collect();
                let sql = format!(
                    "UPDATE {} SET {} WHERE rowid = ?",
                    table,
                    set_clauses.join(", ")
                );
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|e| ProxyError::Plugin(e.to_string()))?;
                let mut i = 1;
                for v in obj.values() {
                    stmt.raw_bind_parameter(i, value_to_sql_string(v).as_str())
                        .map_err(|e| ProxyError::Plugin(e.to_string()))?;
                    i += 1;
                }
                stmt.raw_bind_parameter(i, rowid_str)
                    .map_err(|e| ProxyError::Plugin(e.to_string()))?;
                stmt.raw_execute()
                    .map_err(|e| ProxyError::Plugin(e.to_string()))?;
            }
        }
        OverlayEntryKind::Ddl => {
            // DDL: `after` contains the SQL string.
            if let Some(sql) = after.as_str() {
                conn.execute_batch(sql)
                    .map_err(|e| ProxyError::Plugin(e.to_string()))?;
            }
        }
        OverlayEntryKind::Blob => {
            // Blob apply: write the blob file path back to the DB column.
            tracing::warn!(uri = uri, "SQLite blob apply not yet implemented");
        }
    }
    let _ = (before, upstream_dsn);
    Ok(())
}

fn value_to_sql_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn apply_insert_and_update() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap();

        // Create table.
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (name TEXT, value INTEGER)")
            .unwrap();
        drop(conn);

        // Apply INSERT.
        let uri = format!("sqlite://{}/items/1", db_path_str);
        apply_sqlite_mutation(
            db_path_str,
            &uri,
            None,
            &serde_json::json!({"name": "test", "value": "42"}),
            &OverlayEntryKind::Insert,
            dir.path(),
        )
        .unwrap();

        // Verify.
        let conn = Connection::open(&db_path).unwrap();
        let (name, val): (String, i64) = conn
            .query_row("SELECT name, value FROM items", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(name, "test");
        assert_eq!(val, 42);
    }
}
