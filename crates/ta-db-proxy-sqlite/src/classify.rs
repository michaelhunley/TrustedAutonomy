use ta_db_proxy::classification::{MutationKind, QueryClass};

/// Classify a SQLite SQL statement.
pub fn classify_sqlite_query(sql: &str) -> QueryClass {
    let upper = sql.trim().to_uppercase();
    if upper.starts_with("SELECT")
        || upper.starts_with("EXPLAIN")
        || (upper.starts_with("PRAGMA ") && !upper.contains('='))
    {
        QueryClass::Read
    } else if upper.starts_with("INSERT") {
        QueryClass::Write(MutationKind::Insert)
    } else if upper.starts_with("UPDATE") {
        QueryClass::Write(MutationKind::Update)
    } else if upper.starts_with("DELETE") {
        QueryClass::Write(MutationKind::Delete)
    } else if upper.starts_with("REPLACE") || upper.starts_with("UPSERT") {
        QueryClass::Write(MutationKind::Upsert)
    } else if upper.starts_with("CREATE") || upper.starts_with("ALTER") || upper.starts_with("DROP")
    {
        QueryClass::Ddl
    } else if upper.starts_with("PRAGMA")
        || upper.starts_with("VACUUM")
        || upper.starts_with("ATTACH")
        || upper.starts_with("DETACH")
        || upper.starts_with("BEGIN")
        || upper.starts_with("COMMIT")
        || upper.starts_with("ROLLBACK")
    {
        QueryClass::Admin
    } else {
        QueryClass::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_is_read() {
        assert_eq!(classify_sqlite_query("SELECT * FROM t"), QueryClass::Read);
    }

    #[test]
    fn insert_is_write() {
        assert_eq!(
            classify_sqlite_query("INSERT INTO t VALUES (1)"),
            QueryClass::Write(MutationKind::Insert)
        );
    }

    #[test]
    fn update_is_write() {
        assert_eq!(
            classify_sqlite_query("UPDATE t SET v=1"),
            QueryClass::Write(MutationKind::Update)
        );
    }

    #[test]
    fn create_table_is_ddl() {
        assert_eq!(
            classify_sqlite_query("CREATE TABLE t (id INTEGER)"),
            QueryClass::Ddl
        );
    }

    #[test]
    fn vacuum_is_admin() {
        assert_eq!(classify_sqlite_query("VACUUM"), QueryClass::Admin);
    }
}
