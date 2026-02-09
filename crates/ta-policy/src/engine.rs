// engine.rs — Policy evaluation engine.
//
// The PolicyEngine is the core of the "default deny" security model.
// Every tool call request passes through `evaluate()` which checks:
//
// 1. Does the agent have a manifest? → No → Deny
// 2. Is the manifest expired? → Yes → Deny
// 3. Does the target URI contain path traversal? → Yes → Deny
// 4. Is the verb "apply" (or "commit"/"send"/"post")? → Yes → RequireApproval
// 5. Does any grant match the tool + verb + resource pattern? → Yes → Allow
// 6. No match → Deny
//
// This is deliberately conservative. Future phases can add more sophisticated
// policy rules (role templates, budget tracking, etc.) but the default-deny
// invariant must always hold.

use std::collections::HashMap;

use glob::Pattern;
use serde::{Deserialize, Serialize};

use crate::capability::CapabilityManifest;

/// A request to perform an action — submitted to the policy engine for evaluation.
#[derive(Debug, Clone)]
pub struct PolicyRequest {
    /// Which agent is requesting the action.
    pub agent_id: String,
    /// Which tool/connector (e.g., "fs", "web").
    pub tool: String,
    /// What action (e.g., "read", "write_patch", "apply").
    pub verb: String,
    /// The target resource URI (e.g., "fs://workspace/src/main.rs").
    pub target_uri: String,
}

/// The result of a policy evaluation.
///
/// `#[derive(PartialEq)]` lets us use `==` to compare decisions in tests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PolicyDecision {
    /// The action is allowed — proceed.
    Allow,
    /// The action is denied — do not proceed.
    Deny { reason: String },
    /// The action requires explicit human approval before proceeding.
    RequireApproval { reason: String },
}

/// Verbs that always require human approval, regardless of grants.
/// These represent irreversible side effects.
const APPROVAL_REQUIRED_VERBS: &[&str] = &["apply", "commit", "send", "post"];

/// The policy engine — evaluates requests against capability manifests.
///
/// `HashMap` is Rust's hash map type. We map agent_id → manifest.
pub struct PolicyEngine {
    manifests: HashMap<String, CapabilityManifest>,
}

impl PolicyEngine {
    /// Create a new empty policy engine (denies everything by default).
    pub fn new() -> Self {
        Self {
            manifests: HashMap::new(),
        }
    }

    /// Load a capability manifest for an agent.
    ///
    /// Overwrites any existing manifest for the same agent_id.
    pub fn load_manifest(&mut self, manifest: CapabilityManifest) {
        self.manifests.insert(manifest.agent_id.clone(), manifest);
    }

    /// Evaluate a policy request and return a decision.
    ///
    /// This is the single chokepoint — every tool call flows through here.
    pub fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Step 1: Check for path traversal in the target URI.
        // This is a security check — agents must not escape their workspace.
        if contains_path_traversal(&request.target_uri) {
            return PolicyDecision::Deny {
                reason: format!(
                    "path traversal detected in target URI: '{}'",
                    request.target_uri
                ),
            };
        }

        // Step 2: Look up the agent's manifest.
        let manifest = match self.manifests.get(&request.agent_id) {
            Some(m) => m,
            None => {
                return PolicyDecision::Deny {
                    reason: format!("no capability manifest for agent '{}'", request.agent_id),
                }
            }
        };

        // Step 3: Check if the manifest has expired.
        if manifest.is_expired() {
            return PolicyDecision::Deny {
                reason: format!(
                    "capability manifest for agent '{}' has expired",
                    request.agent_id
                ),
            };
        }

        // Step 4: Check if this verb always requires approval.
        if APPROVAL_REQUIRED_VERBS.contains(&request.verb.as_str()) {
            // Still need to verify the agent has a matching grant.
            if has_matching_grant(manifest, request) {
                return PolicyDecision::RequireApproval {
                    reason: format!("verb '{}' requires explicit approval", request.verb),
                };
            } else {
                return PolicyDecision::Deny {
                    reason: format!(
                        "no grant for {}.{} on '{}'",
                        request.tool, request.verb, request.target_uri
                    ),
                };
            }
        }

        // Step 5: Check if any grant matches.
        if has_matching_grant(manifest, request) {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Deny {
                reason: format!(
                    "no grant for {}.{} on '{}'",
                    request.tool, request.verb, request.target_uri
                ),
            }
        }
    }
}

/// Implement Default for PolicyEngine so it can be created with PolicyEngine::default().
impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if any grant in the manifest matches the request.
///
/// A grant matches if:
/// - grant.tool == request.tool
/// - grant.verb == request.verb
/// - grant.resource_pattern (as a glob) matches request.target_uri
fn has_matching_grant(manifest: &CapabilityManifest, request: &PolicyRequest) -> bool {
    manifest.grants.iter().any(|grant| {
        grant.tool == request.tool
            && grant.verb == request.verb
            && matches_resource_pattern(&grant.resource_pattern, &request.target_uri)
    })
}

/// Check if a glob pattern matches a target URI.
///
/// Uses the `glob` crate for pattern matching. If the pattern is invalid,
/// it does not match (fail-closed, not fail-open).
fn matches_resource_pattern(pattern: &str, target: &str) -> bool {
    match Pattern::new(pattern) {
        Ok(p) => p.matches(target),
        Err(_) => false, // Invalid patterns never match (fail-closed)
    }
}

/// Detect path traversal attempts in URIs.
///
/// Checks for ".." sequences that could escape the intended scope.
fn contains_path_traversal(uri: &str) -> bool {
    // Check for various path traversal patterns.
    // We check the raw string rather than path-normalizing, because we want
    // to catch all encoding tricks.
    uri.contains("..") || uri.contains("%2e%2e") || uri.contains("%2E%2E")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityGrant, CapabilityManifest};
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    /// Helper: create a manifest with the given grants, valid for 1 hour.
    fn test_manifest(agent_id: &str, grants: Vec<CapabilityGrant>) -> CapabilityManifest {
        CapabilityManifest {
            manifest_id: Uuid::new_v4(),
            agent_id: agent_id.to_string(),
            grants,
            issued_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        }
    }

    /// Helper: create a grant for a tool/verb/pattern.
    fn grant(tool: &str, verb: &str, pattern: &str) -> CapabilityGrant {
        CapabilityGrant {
            tool: tool.to_string(),
            verb: verb.to_string(),
            resource_pattern: pattern.to_string(),
        }
    }

    #[test]
    fn allow_granted_action() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/**")],
        ));

        let decision = engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/src/main.rs".to_string(),
        });

        assert_eq!(decision, PolicyDecision::Allow);
    }

    #[test]
    fn deny_when_no_matching_grant() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/**")],
        ));

        let decision = engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "write_patch".to_string(), // not granted
            target_uri: "fs://workspace/src/main.rs".to_string(),
        });

        match decision {
            PolicyDecision::Deny { .. } => {} // expected
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn deny_unknown_agent() {
        let engine = PolicyEngine::new(); // no manifests loaded

        let decision = engine.evaluate(&PolicyRequest {
            agent_id: "unknown-agent".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/test.txt".to_string(),
        });

        match decision {
            PolicyDecision::Deny { reason } => {
                assert!(reason.contains("no capability manifest"));
            }
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn deny_expired_manifest() {
        let mut engine = PolicyEngine::new();
        let mut manifest = test_manifest("agent-1", vec![grant("fs", "read", "fs://workspace/**")]);
        // Set manifest to already be expired
        manifest.issued_at = Utc::now() - Duration::hours(2);
        manifest.expires_at = Utc::now() - Duration::hours(1);
        engine.load_manifest(manifest);

        let decision = engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/test.txt".to_string(),
        });

        match decision {
            PolicyDecision::Deny { reason } => {
                assert!(reason.contains("expired"));
            }
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn apply_always_requires_approval() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "apply", "fs://workspace/**")],
        ));

        let decision = engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "apply".to_string(),
            target_uri: "fs://workspace/src/main.rs".to_string(),
        });

        match decision {
            PolicyDecision::RequireApproval { .. } => {} // expected
            other => panic!("expected RequireApproval, got {:?}", other),
        }
    }

    #[test]
    fn commit_requires_approval() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "commit", "fs://workspace/**")],
        ));

        let decision = engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "commit".to_string(),
            target_uri: "fs://workspace/test.txt".to_string(),
        });

        match decision {
            PolicyDecision::RequireApproval { .. } => {} // expected
            other => panic!("expected RequireApproval, got {:?}", other),
        }
    }

    #[test]
    fn deny_path_traversal() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/**")],
        ));

        let decision = engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/../etc/passwd".to_string(),
        });

        match decision {
            PolicyDecision::Deny { reason } => {
                assert!(reason.contains("path traversal"));
            }
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn glob_wildcard_matching() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/src/**")],
        ));

        // Should match: inside src/
        assert_eq!(
            engine.evaluate(&PolicyRequest {
                agent_id: "agent-1".to_string(),
                tool: "fs".to_string(),
                verb: "read".to_string(),
                target_uri: "fs://workspace/src/lib.rs".to_string(),
            }),
            PolicyDecision::Allow
        );

        // Should NOT match: outside src/
        match engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/Cargo.toml".to_string(),
        }) {
            PolicyDecision::Deny { .. } => {} // expected
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn exact_resource_pattern() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/specific-file.txt")],
        ));

        // Exact match should work
        assert_eq!(
            engine.evaluate(&PolicyRequest {
                agent_id: "agent-1".to_string(),
                tool: "fs".to_string(),
                verb: "read".to_string(),
                target_uri: "fs://workspace/specific-file.txt".to_string(),
            }),
            PolicyDecision::Allow
        );

        // Different file should be denied
        match engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/other-file.txt".to_string(),
        }) {
            PolicyDecision::Deny { .. } => {} // expected
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn multiple_grants_any_match_allows() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![
                grant("fs", "read", "fs://workspace/**"),
                grant("fs", "write_patch", "fs://workspace/src/**"),
            ],
        ));

        // Read anything in workspace → Allow
        assert_eq!(
            engine.evaluate(&PolicyRequest {
                agent_id: "agent-1".to_string(),
                tool: "fs".to_string(),
                verb: "read".to_string(),
                target_uri: "fs://workspace/Cargo.toml".to_string(),
            }),
            PolicyDecision::Allow
        );

        // Write in src/ → Allow
        assert_eq!(
            engine.evaluate(&PolicyRequest {
                agent_id: "agent-1".to_string(),
                tool: "fs".to_string(),
                verb: "write_patch".to_string(),
                target_uri: "fs://workspace/src/main.rs".to_string(),
            }),
            PolicyDecision::Allow
        );

        // Write outside src/ → Deny
        match engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "write_patch".to_string(),
            target_uri: "fs://workspace/Cargo.toml".to_string(),
        }) {
            PolicyDecision::Deny { .. } => {} // expected
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn apply_denied_without_grant() {
        let mut engine = PolicyEngine::new();
        // Agent has read but NOT apply
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/**")],
        ));

        let decision = engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "apply".to_string(),
            target_uri: "fs://workspace/test.txt".to_string(),
        });

        // Should be Deny (not RequireApproval) because there's no grant for apply
        match decision {
            PolicyDecision::Deny { .. } => {} // expected
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn different_tools_are_separate() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest("agent-1", vec![grant("fs", "read", "**")]));

        // fs.read → Allow
        assert_eq!(
            engine.evaluate(&PolicyRequest {
                agent_id: "agent-1".to_string(),
                tool: "fs".to_string(),
                verb: "read".to_string(),
                target_uri: "fs://workspace/test.txt".to_string(),
            }),
            PolicyDecision::Allow
        );

        // web.read → Deny (different tool, not granted)
        match engine.evaluate(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "web".to_string(),
            verb: "read".to_string(),
            target_uri: "web://example.com".to_string(),
        }) {
            PolicyDecision::Deny { .. } => {} // expected
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn decision_serialization() {
        // Verify decisions serialize properly for audit logging.
        let allow = PolicyDecision::Allow;
        let json = serde_json::to_string(&allow).unwrap();
        assert!(json.contains("\"allow\""));

        let deny = PolicyDecision::Deny {
            reason: "test".to_string(),
        };
        let json = serde_json::to_string(&deny).unwrap();
        assert!(json.contains("\"deny\""));
    }
}
