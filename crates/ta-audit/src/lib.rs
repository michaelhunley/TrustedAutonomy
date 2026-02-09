//! # ta-audit
//!
//! Append-only event log and artifact hashing for Trusted Autonomy.
//!
//! Every tool call, policy decision, approval, and commit in the system
//! is recorded as an [`AuditEvent`] in a JSONL (JSON Lines) log file.
//! Each event includes SHA-256 hashes of its inputs and outputs for
//! tamper detection and replay verification.

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // Stub test — will be replaced with real tests during implementation.
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
