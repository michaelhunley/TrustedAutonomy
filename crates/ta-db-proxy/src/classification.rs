use serde::{Deserialize, Serialize};

/// How a database query is classified for policy enforcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryClass {
    /// Read-only operation (SELECT, EXPLAIN, etc.)
    Read,
    /// Data mutation (INSERT, UPDATE, DELETE, UPSERT)
    Write(MutationKind),
    /// Schema change (CREATE TABLE, ALTER TABLE, DROP, etc.)
    Ddl,
    /// Administrative operation (VACUUM, PRAGMA, transaction control, etc.)
    Admin,
    /// Query could not be classified (passthrough with logging).
    Unknown,
}

/// The specific kind of mutation for WRITE queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Insert,
    Update,
    Delete,
    Upsert,
}
