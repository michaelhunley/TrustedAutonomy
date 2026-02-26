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

/// A step in the policy evaluation chain (v0.3.3).
///
/// Captures what the engine checked at each stage so the decision trail
/// is fully observable for compliance reporting and drift detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationStep {
    /// Which check was performed (e.g., "path_traversal", "manifest_lookup").
    pub check: String,
    /// The outcome of this check (e.g., "passed", "failed: expired").
    pub outcome: String,
    /// Whether this step was the terminal decision point.
    pub terminal: bool,
}

/// Full evaluation trace returned alongside a PolicyDecision (v0.3.3).
///
/// Records every check performed by `PolicyEngine::evaluate()`, which grants
/// were inspected, and which matched — enabling full decision observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationTrace {
    /// The final decision.
    pub decision: PolicyDecision,
    /// Ordered steps the engine evaluated.
    pub steps: Vec<EvaluationStep>,
    /// Which grants were checked (tool.verb on pattern).
    pub grants_checked: Vec<String>,
    /// Which grant matched (if any).
    pub matching_grant: Option<String>,
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

    /// Evaluate a policy request and return the decision with a full trace (v0.3.3).
    ///
    /// Same logic as `evaluate()` but records every step for decision observability.
    pub fn evaluate_with_trace(&self, request: &PolicyRequest) -> EvaluationTrace {
        let mut steps = Vec::new();
        let mut grants_checked = Vec::new();
        let mut matching_grant = None;

        // Step 1: Path traversal
        if contains_path_traversal(&request.target_uri) {
            steps.push(EvaluationStep {
                check: "path_traversal".to_string(),
                outcome: format!("failed: traversal detected in '{}'", request.target_uri),
                terminal: true,
            });
            return EvaluationTrace {
                decision: PolicyDecision::Deny {
                    reason: format!(
                        "path traversal detected in target URI: '{}'",
                        request.target_uri
                    ),
                },
                steps,
                grants_checked,
                matching_grant,
            };
        }
        steps.push(EvaluationStep {
            check: "path_traversal".to_string(),
            outcome: "passed".to_string(),
            terminal: false,
        });

        // Step 2: Manifest lookup
        let manifest = match self.manifests.get(&request.agent_id) {
            Some(m) => m,
            None => {
                steps.push(EvaluationStep {
                    check: "manifest_lookup".to_string(),
                    outcome: format!("failed: no manifest for '{}'", request.agent_id),
                    terminal: true,
                });
                return EvaluationTrace {
                    decision: PolicyDecision::Deny {
                        reason: format!("no capability manifest for agent '{}'", request.agent_id),
                    },
                    steps,
                    grants_checked,
                    matching_grant,
                };
            }
        };
        steps.push(EvaluationStep {
            check: "manifest_lookup".to_string(),
            outcome: format!(
                "found: {} grants, expires {}",
                manifest.grants.len(),
                manifest.expires_at
            ),
            terminal: false,
        });

        // Step 3: Expiry
        if manifest.is_expired() {
            steps.push(EvaluationStep {
                check: "manifest_expiry".to_string(),
                outcome: "failed: manifest expired".to_string(),
                terminal: true,
            });
            return EvaluationTrace {
                decision: PolicyDecision::Deny {
                    reason: format!(
                        "capability manifest for agent '{}' has expired",
                        request.agent_id
                    ),
                },
                steps,
                grants_checked,
                matching_grant,
            };
        }
        steps.push(EvaluationStep {
            check: "manifest_expiry".to_string(),
            outcome: "passed".to_string(),
            terminal: false,
        });

        // Collect grant check details
        for grant in &manifest.grants {
            let desc = format!(
                "{}.{} on '{}'",
                grant.tool, grant.verb, grant.resource_pattern
            );
            grants_checked.push(desc.clone());
            if grant.tool == request.tool
                && grant.verb == request.verb
                && matches_resource_pattern(&grant.resource_pattern, &request.target_uri)
            {
                matching_grant = Some(desc);
            }
        }

        // Step 4: Approval-required verbs
        if APPROVAL_REQUIRED_VERBS.contains(&request.verb.as_str()) {
            if matching_grant.is_some() {
                steps.push(EvaluationStep {
                    check: "approval_required_verb".to_string(),
                    outcome: format!(
                        "verb '{}' requires approval; matching grant found",
                        request.verb
                    ),
                    terminal: true,
                });
                return EvaluationTrace {
                    decision: PolicyDecision::RequireApproval {
                        reason: format!("verb '{}' requires explicit approval", request.verb),
                    },
                    steps,
                    grants_checked,
                    matching_grant,
                };
            } else {
                steps.push(EvaluationStep {
                    check: "approval_required_verb".to_string(),
                    outcome: format!(
                        "verb '{}' requires approval; no matching grant",
                        request.verb
                    ),
                    terminal: true,
                });
                return EvaluationTrace {
                    decision: PolicyDecision::Deny {
                        reason: format!(
                            "no grant for {}.{} on '{}'",
                            request.tool, request.verb, request.target_uri
                        ),
                    },
                    steps,
                    grants_checked,
                    matching_grant,
                };
            }
        }

        // Step 5: Grant matching
        if matching_grant.is_some() {
            steps.push(EvaluationStep {
                check: "grant_match".to_string(),
                outcome: "allowed: matching grant found".to_string(),
                terminal: true,
            });
            EvaluationTrace {
                decision: PolicyDecision::Allow,
                steps,
                grants_checked,
                matching_grant,
            }
        } else {
            steps.push(EvaluationStep {
                check: "grant_match".to_string(),
                outcome: format!(
                    "denied: no grant for {}.{} on '{}'",
                    request.tool, request.verb, request.target_uri
                ),
                terminal: true,
            });
            EvaluationTrace {
                decision: PolicyDecision::Deny {
                    reason: format!(
                        "no grant for {}.{} on '{}'",
                        request.tool, request.verb, request.target_uri
                    ),
                },
                steps,
                grants_checked,
                matching_grant,
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

    // ── v0.3.3 Evaluation Trace tests ──

    #[test]
    fn trace_records_allow_steps() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/**")],
        ));

        let trace = engine.evaluate_with_trace(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/src/main.rs".to_string(),
        });

        assert_eq!(trace.decision, PolicyDecision::Allow);
        // Should have path_traversal, manifest_lookup, manifest_expiry, grant_match steps.
        assert!(trace.steps.len() >= 4);
        assert_eq!(trace.steps[0].check, "path_traversal");
        assert!(!trace.steps[0].terminal);
        assert!(trace.steps.last().unwrap().terminal);
        assert_eq!(trace.grants_checked.len(), 1);
        assert!(trace.matching_grant.is_some());
    }

    #[test]
    fn trace_records_deny_no_manifest() {
        let engine = PolicyEngine::new();

        let trace = engine.evaluate_with_trace(&PolicyRequest {
            agent_id: "unknown".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/test.txt".to_string(),
        });

        match &trace.decision {
            PolicyDecision::Deny { reason } => assert!(reason.contains("no capability manifest")),
            other => panic!("expected Deny, got {:?}", other),
        }
        assert_eq!(trace.steps.len(), 2); // path_traversal + manifest_lookup
        assert!(trace.steps[1].terminal);
    }

    #[test]
    fn trace_records_path_traversal() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/**")],
        ));

        let trace = engine.evaluate_with_trace(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/../etc/passwd".to_string(),
        });

        match &trace.decision {
            PolicyDecision::Deny { reason } => assert!(reason.contains("path traversal")),
            other => panic!("expected Deny, got {:?}", other),
        }
        assert_eq!(trace.steps.len(), 1);
        assert!(trace.steps[0].terminal);
    }

    #[test]
    fn trace_records_approval_required() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "apply", "fs://workspace/**")],
        ));

        let trace = engine.evaluate_with_trace(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "apply".to_string(),
            target_uri: "fs://workspace/src/main.rs".to_string(),
        });

        match &trace.decision {
            PolicyDecision::RequireApproval { .. } => {}
            other => panic!("expected RequireApproval, got {:?}", other),
        }
        assert!(trace.matching_grant.is_some());
    }

    #[test]
    fn trace_lists_all_grants_checked() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![
                grant("fs", "read", "fs://workspace/**"),
                grant("fs", "write_patch", "fs://workspace/src/**"),
                grant("web", "fetch", "https://**"),
            ],
        ));

        let trace = engine.evaluate_with_trace(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/test.txt".to_string(),
        });

        // All 3 grants should be listed as checked.
        assert_eq!(trace.grants_checked.len(), 3);
    }

    #[test]
    fn trace_serialization_round_trip() {
        let mut engine = PolicyEngine::new();
        engine.load_manifest(test_manifest(
            "agent-1",
            vec![grant("fs", "read", "fs://workspace/**")],
        ));

        let trace = engine.evaluate_with_trace(&PolicyRequest {
            agent_id: "agent-1".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/file.txt".to_string(),
        });

        let json = serde_json::to_string(&trace).unwrap();
        let restored: EvaluationTrace = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.decision, trace.decision);
        assert_eq!(restored.steps.len(), trace.steps.len());
        assert_eq!(restored.grants_checked.len(), trace.grants_checked.len());
    }
}
