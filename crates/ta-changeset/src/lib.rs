//! # ta-changeset
//!
//! The universal "staged mutation" data model for Trusted Autonomy.
//!
//! A [`ChangeSet`] represents any pending change — a file patch, email draft,
//! DB mutation, or social media post. All changes are collected (staged) by
//! default and bundled into a [`PRPackage`] for human review.
//!
//! The data model aligns with `schema/pr_package.schema.json`.

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // Stub test — will be replaced with real tests during implementation.
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
