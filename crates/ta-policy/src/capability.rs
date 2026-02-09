// capability.rs — Capability manifest and grant definitions.
//
// A capability manifest is issued per agent per goal iteration.
// It lists exactly what the agent is allowed to do (tool + verb + resource
// pattern). This is the "default deny" mechanism: if it's not in the
// manifest, it's denied.
//
// Manifests are time-bounded to limit blast radius of compromised agents.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single permission grant within a capability manifest.
///
/// Grants are scoped by three dimensions:
/// - `tool`: which connector (e.g., "fs", "web", "gmail")
/// - `verb`: what action (e.g., "read", "write_patch", "apply")
/// - `resource_pattern`: a glob pattern for target URIs
///
/// Example grant: { tool: "fs", verb: "read", resource_pattern: "fs://workspace/**" }
/// This allows reading any file under the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityGrant {
    /// The tool/connector this grant applies to.
    pub tool: String,
    /// The action verb this grant permits.
    pub verb: String,
    /// Glob pattern matching target URIs (e.g., "fs://workspace/**").
    pub resource_pattern: String,
}

/// A capability manifest — the complete set of permissions for one agent.
///
/// Issued at the start of a goal iteration and time-bounded.
/// In a future phase, manifests will be cryptographically signed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    /// Unique ID for this manifest.
    pub manifest_id: Uuid,
    /// The agent this manifest is issued to.
    pub agent_id: String,
    /// The set of permissions granted.
    pub grants: Vec<CapabilityGrant>,
    /// When this manifest was issued.
    pub issued_at: DateTime<Utc>,
    /// When this manifest expires (hard cutoff).
    pub expires_at: DateTime<Utc>,
}

impl CapabilityManifest {
    /// Check if this manifest has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn manifest_not_expired_when_fresh() {
        let manifest = CapabilityManifest {
            manifest_id: Uuid::new_v4(),
            agent_id: "test-agent".to_string(),
            grants: vec![],
            issued_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        assert!(!manifest.is_expired());
    }

    #[test]
    fn manifest_expired_when_past_deadline() {
        let manifest = CapabilityManifest {
            manifest_id: Uuid::new_v4(),
            agent_id: "test-agent".to_string(),
            grants: vec![],
            issued_at: Utc::now() - Duration::hours(2),
            expires_at: Utc::now() - Duration::hours(1),
        };
        assert!(manifest.is_expired());
    }

    #[test]
    fn grant_serialization_round_trip() {
        let grant = CapabilityGrant {
            tool: "fs".to_string(),
            verb: "read".to_string(),
            resource_pattern: "fs://workspace/**".to_string(),
        };
        let json = serde_json::to_string(&grant).unwrap();
        let restored: CapabilityGrant = serde_json::from_str(&json).unwrap();
        assert_eq!(grant, restored);
    }
}
