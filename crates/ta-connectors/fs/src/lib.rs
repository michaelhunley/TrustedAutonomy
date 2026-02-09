//! # ta-connector-fs
//!
//! Filesystem connector for Trusted Autonomy.
//!
//! Bridges MCP-style tool operations (read, write_patch, diff) to the
//! staging workspace and changeset model. All writes go to a staging
//! directory; approved changes are applied to the real target via `apply()`.

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // Stub test — will be replaced with real tests during implementation.
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
