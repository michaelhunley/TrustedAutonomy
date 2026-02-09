//! # ta-policy
//!
//! Capability-based policy engine for Trusted Autonomy.
//!
//! Implements the "default deny" security boundary: agents can only perform
//! actions explicitly granted by a [`CapabilityManifest`]. The [`PolicyEngine`]
//! evaluates each tool call request against the agent's grants and returns
//! Allow, Deny, or RequireApproval.

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // Stub test — will be replaced with real tests during implementation.
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
