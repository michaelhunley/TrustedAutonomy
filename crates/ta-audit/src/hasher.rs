// hasher.rs — SHA-256 hashing utilities.
//
// All hashes in Trusted Autonomy are SHA-256, hex-encoded. This module
// provides convenience functions for hashing bytes, strings, and files.
//
// SHA-256 produces a 32-byte (256-bit) digest. We encode it as a 64-character
// lowercase hex string for readability and JSON compatibility.

use sha2::{Digest, Sha256};
use std::path::Path;

use crate::error::AuditError;

/// Hash arbitrary bytes, returning a lowercase hex-encoded SHA-256 string.
///
/// This is deterministic: the same input always produces the same output.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    // `format!("{:x}", ...)` produces lowercase hex
    format!("{:x}", result)
}

/// Hash a UTF-8 string, returning a lowercase hex-encoded SHA-256 string.
pub fn hash_str(s: &str) -> String {
    hash_bytes(s.as_bytes())
}

/// Hash the contents of a file on disk.
///
/// Reads the entire file into memory. For very large files, a streaming
/// approach would be better, but this is sufficient for the MVP.
pub fn hash_file(path: &Path) -> Result<String, AuditError> {
    let data = std::fs::read(path).map_err(|source| AuditError::HashFileFailed {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(hash_bytes(&data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_determinism() {
        // Same input must always produce the same hash.
        let input = b"hello world";
        let hash1 = hash_bytes(input);
        let hash2 = hash_bytes(input);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_uniqueness() {
        // Different inputs must produce different hashes.
        let hash1 = hash_bytes(b"hello");
        let hash2 = hash_bytes(b"world");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_is_hex_encoded_sha256() {
        // SHA-256 produces a 64-character hex string.
        let hash = hash_str("test");
        assert_eq!(hash.len(), 64);
        // All characters should be lowercase hex
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn hash_known_value() {
        // Verify against a known SHA-256 value.
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let hash = hash_str("");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hash_str_matches_hash_bytes() {
        let s = "hello world";
        assert_eq!(hash_str(s), hash_bytes(s.as_bytes()));
    }
}
