// attestation.rs — Hardware-agnostic audit log attestation (v0.14.1).
//
// Every audit event can optionally carry an `AttestationRecord` — a
// cryptographic signature over the event payload produced by an
// `AttestationBackend`.  The signature binds the audit record to the
// specific machine (or key) that wrote it, making retroactive forgery
// detectable without a hardware TPM.
//
// ## Signing protocol
//
// 1. The event is serialized to JSON with `attestation: null` (canonical form).
// 2. The backend signs those canonical bytes.
// 3. The resulting `AttestationRecord` is attached to the event.
//
// Verification reverses step 1–3: set `attestation = None`, serialize, verify.
//
// ## Backends
//
// | Backend               | Key storage                     | Platform |
// |-----------------------|---------------------------------|----------|
// | `SoftwareBackend`     | `.ta/keys/attestation.pkcs8`    | All      |
// | TPM 2.0 (future)      | TPM NV storage                  | Linux/Win|
// | Apple SE (future)     | macOS Keychain                  | macOS    |
//
// All future backends implement `AttestationBackend` and are discovered via
// the plugin registry (v0.14.3).

use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ring::{
    rand::SystemRandom,
    signature::{self, Ed25519KeyPair, KeyPair, UnparsedPublicKey},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Error ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AttestationError {
    #[error("Key generation failed: {0}")]
    KeyGen(String),
    #[error("Failed to read key file {path}: {source}")]
    KeyRead {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to write key file {path}: {source}")]
    KeyWrite {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("Invalid PKCS8 key data: {0}")]
    InvalidKey(String),
    #[error("Signature verification failed")]
    VerificationFailed,
    #[error("Invalid base64 signature: {0}")]
    InvalidSignature(String),
}

// ─── AttestationRecord ───────────────────────────────────────────────────────

/// A cryptographic signature record attached to an audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationRecord {
    /// Name of the backend that produced this record (e.g., "software-ed25519").
    pub backend: String,
    /// First 8 hex bytes of SHA-256(public_key) — identifies the signing key.
    pub key_fingerprint: String,
    /// Base64-encoded Ed25519 signature over the canonical event bytes.
    pub signature: String,
}

// ─── AttestationBackend trait ────────────────────────────────────────────────

/// A source of cryptographic signatures for audit events.
///
/// Implementations: `SoftwareAttestationBackend` (ships with TA), TPM 2.0
/// plugin (future), Apple Secure Enclave plugin (future).
pub trait AttestationBackend: Send + Sync {
    /// Human-readable backend identifier (e.g., "software-ed25519").
    fn name(&self) -> &str;

    /// Short fingerprint of the public key (first 16 hex chars of SHA-256).
    fn public_key_fingerprint(&self) -> String;

    /// Sign `payload` and return an `AttestationRecord`.
    fn sign(&self, payload: &[u8]) -> Result<AttestationRecord, AttestationError>;

    /// Verify that `record` is a valid signature over `payload` by this backend.
    fn verify(&self, payload: &[u8], record: &AttestationRecord) -> Result<bool, AttestationError>;
}

// ─── SoftwareAttestationBackend ──────────────────────────────────────────────

/// Ed25519 software attestation backend.
///
/// Stores the private key as a PKCS8 DER file in `.ta/keys/attestation.pkcs8`.
/// The public key (raw 32 bytes, hex-encoded) is written alongside it as
/// `.ta/keys/attestation.pub` for out-of-band distribution.
///
/// The key is auto-generated on first use.
pub struct SoftwareAttestationBackend {
    key_pair: Ed25519KeyPair,
    public_key_bytes: Vec<u8>,
}

impl SoftwareAttestationBackend {
    /// Load an existing key from `keys_dir`, or generate a new one if absent.
    ///
    /// Key files created:
    /// - `<keys_dir>/attestation.pkcs8` — PKCS8 DER private key (binary)
    /// - `<keys_dir>/attestation.pub`   — hex-encoded public key (text)
    pub fn load_or_generate(keys_dir: &Path) -> Result<Self, AttestationError> {
        let pkcs8_path = keys_dir.join("attestation.pkcs8");
        let pub_path = keys_dir.join("attestation.pub");

        if pkcs8_path.exists() {
            // Load existing key.
            let pkcs8_bytes =
                std::fs::read(&pkcs8_path).map_err(|source| AttestationError::KeyRead {
                    path: pkcs8_path.clone(),
                    source,
                })?;
            let key_pair = Ed25519KeyPair::from_pkcs8(&pkcs8_bytes)
                .map_err(|e| AttestationError::InvalidKey(e.to_string()))?;
            let public_key_bytes = key_pair.public_key().as_ref().to_vec();
            Ok(Self {
                key_pair,
                public_key_bytes,
            })
        } else {
            // Generate new key.
            std::fs::create_dir_all(keys_dir).map_err(|source| AttestationError::KeyWrite {
                path: keys_dir.to_path_buf(),
                source,
            })?;

            let rng = SystemRandom::new();
            let pkcs8_doc = Ed25519KeyPair::generate_pkcs8(&rng)
                .map_err(|e| AttestationError::KeyGen(e.to_string()))?;

            // Write private key.
            std::fs::write(&pkcs8_path, pkcs8_doc.as_ref()).map_err(|source| {
                AttestationError::KeyWrite {
                    path: pkcs8_path.clone(),
                    source,
                }
            })?;

            let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_doc.as_ref())
                .map_err(|e| AttestationError::InvalidKey(e.to_string()))?;
            let public_key_bytes = key_pair.public_key().as_ref().to_vec();

            // Write public key (hex) for distribution.
            let pub_hex = hex_encode(&public_key_bytes);
            std::fs::write(&pub_path, &pub_hex).map_err(|source| AttestationError::KeyWrite {
                path: pub_path,
                source,
            })?;

            tracing::info!(
                path = ?pkcs8_path,
                fingerprint = %fingerprint_of(&public_key_bytes),
                "Generated new Ed25519 attestation key"
            );

            Ok(Self {
                key_pair,
                public_key_bytes,
            })
        }
    }
}

impl AttestationBackend for SoftwareAttestationBackend {
    fn name(&self) -> &str {
        "software-ed25519"
    }

    fn public_key_fingerprint(&self) -> String {
        fingerprint_of(&self.public_key_bytes)
    }

    fn sign(&self, payload: &[u8]) -> Result<AttestationRecord, AttestationError> {
        let sig = self.key_pair.sign(payload);
        Ok(AttestationRecord {
            backend: self.name().to_string(),
            key_fingerprint: self.public_key_fingerprint(),
            signature: B64.encode(sig.as_ref()),
        })
    }

    fn verify(&self, payload: &[u8], record: &AttestationRecord) -> Result<bool, AttestationError> {
        let sig_bytes = B64
            .decode(&record.signature)
            .map_err(|e| AttestationError::InvalidSignature(e.to_string()))?;
        let pub_key = UnparsedPublicKey::new(&signature::ED25519, self.public_key_bytes.as_slice());
        match pub_key.verify(payload, &sig_bytes) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Compute the key fingerprint: first 16 hex characters of SHA-256(pubkey).
pub fn fingerprint_of(public_key_bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(public_key_bytes);
    let result = hasher.finalize();
    hex_encode(&result[..8])
}

/// Hex-encode bytes as a lowercase string.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Produce the canonical bytes for signing an `AuditEvent`.
///
/// The canonical form is the JSON of the event with `attestation` set to
/// `null`, so the field is always present (deterministic ordering) but
/// carries no signature data.  This prevents a chicken-and-egg problem where
/// the signature would need to include itself.
///
/// Caller must pass the event already serialized with `attestation: None`.
/// This function is a no-op convenience wrapper — the contract is enforced
/// at the call site in `AuditLog::append`.
pub fn canonical_bytes(event_json_with_null_attestation: &str) -> Vec<u8> {
    event_json_with_null_attestation.as_bytes().to_vec()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_backend(dir: &TempDir) -> SoftwareAttestationBackend {
        SoftwareAttestationBackend::load_or_generate(dir.path()).unwrap()
    }

    #[test]
    fn generate_creates_key_files() {
        let dir = TempDir::new().unwrap();
        let _ = make_backend(&dir);
        assert!(
            dir.path().join("attestation.pkcs8").exists(),
            "private key file should exist"
        );
        assert!(
            dir.path().join("attestation.pub").exists(),
            "public key file should exist"
        );
    }

    #[test]
    fn load_reuses_existing_key() {
        let dir = TempDir::new().unwrap();
        let b1 = make_backend(&dir);
        let fp1 = b1.public_key_fingerprint();
        // Load again — should reuse the same key.
        let b2 = SoftwareAttestationBackend::load_or_generate(dir.path()).unwrap();
        assert_eq!(
            fp1,
            b2.public_key_fingerprint(),
            "should reuse existing key"
        );
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let dir = TempDir::new().unwrap();
        let backend = make_backend(&dir);
        let payload = b"hello audit world";
        let record = backend.sign(payload).unwrap();
        assert!(
            backend.verify(payload, &record).unwrap(),
            "valid signature should verify"
        );
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let dir = TempDir::new().unwrap();
        let backend = make_backend(&dir);
        let record = backend.sign(b"original payload").unwrap();
        assert!(
            !backend.verify(b"tampered payload", &record).unwrap(),
            "tampered payload should fail verification"
        );
    }

    #[test]
    fn corrupted_signature_fails_verification() {
        let dir = TempDir::new().unwrap();
        let backend = make_backend(&dir);
        let mut record = backend.sign(b"some data").unwrap();
        record.signature = B64.encode(b"bad signature bytes of wrong length 00000000000000000000000000000000000000000000000000000000000000000");
        assert!(
            !backend.verify(b"some data", &record).unwrap(),
            "corrupted signature should fail"
        );
    }

    #[test]
    fn fingerprint_is_16_hex_chars() {
        let dir = TempDir::new().unwrap();
        let backend = make_backend(&dir);
        let fp = backend.public_key_fingerprint();
        assert_eq!(fp.len(), 16, "fingerprint should be 16 hex chars");
        assert!(
            fp.chars().all(|c| c.is_ascii_hexdigit()),
            "fingerprint should be hex"
        );
    }
}
