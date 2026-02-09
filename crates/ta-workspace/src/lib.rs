//! # ta-workspace
//!
//! Staging workspace manager for Trusted Autonomy.
//!
//! Manages ephemeral temp directories where agents stage filesystem changes.
//! Changes are tracked via a [`ChangeStore`] trait — the MVP implementation
//! (`JsonFileStore`) persists to JSONL on disk so work is never lost.
//! The trait can be swapped for SQLite or other backends later.

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // Stub test — will be replaced with real tests during implementation.
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
