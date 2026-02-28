// compiler.rs — Policy Compiler (v0.4.0).
//
// Compiles an AlignmentProfile into a CapabilityManifest. This replaces
// the hardcoded manifest generation in ta-mcp-gateway/server.rs.
//
// The compiler:
// 1. Parses bounded_actions into (tool, verb) pairs
// 2. Validates that no forbidden_action overlaps with bounded_actions
// 3. Applies resource_scope patterns (or defaults to workspace/**)
// 4. Generates time-bounded CapabilityManifest with matching grants
//
// The key invariant: if an action is in `forbidden_actions`, it NEVER
// appears in the manifest — this is enforced, not promised.

use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::alignment::AlignmentProfile;
use crate::capability::{CapabilityGrant, CapabilityManifest};

/// Errors that can occur during policy compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerError {
    /// A bounded_action overlaps with a forbidden_action.
    ForbiddenOverlap { action: String, message: String },
    /// A bounded_action has an unrecognized format.
    InvalidAction { action: String, message: String },
}

impl std::fmt::Display for CompilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompilerError::ForbiddenOverlap { action, message } => {
                write!(f, "forbidden overlap for '{}': {}", action, message)
            }
            CompilerError::InvalidAction { action, message } => {
                write!(f, "invalid action '{}': {}", action, message)
            }
        }
    }
}

impl std::error::Error for CompilerError {}

/// Options for the Policy Compiler.
#[derive(Debug, Clone)]
pub struct CompilerOptions {
    /// Resource patterns to scope grants to (defaults to ["fs://workspace/**"]).
    pub resource_scope: Vec<String>,

    /// How long the manifest is valid for (defaults to 8 hours).
    pub validity_hours: i64,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            resource_scope: vec!["fs://workspace/**".to_string()],
            validity_hours: 8,
        }
    }
}

/// The Policy Compiler — transforms alignment profiles into enforceable capability manifests.
pub struct PolicyCompiler;

impl PolicyCompiler {
    /// Compile an AlignmentProfile into a CapabilityManifest.
    ///
    /// This is the core function that replaces hardcoded manifest generation.
    /// It parses bounded_actions, validates against forbidden_actions, and
    /// generates scoped grants.
    pub fn compile(
        agent_id: &str,
        profile: &AlignmentProfile,
        options: &CompilerOptions,
    ) -> Result<CapabilityManifest, CompilerError> {
        // Step 1: Validate no overlaps between bounded and forbidden actions.
        Self::validate_no_overlaps(profile)?;

        // Step 2: Parse bounded_actions into grants.
        let mut grants = Vec::new();
        for action in &profile.autonomy_envelope.bounded_actions {
            let parsed = Self::parse_action(action)?;
            // Generate a grant for each resource scope pattern.
            for pattern in &options.resource_scope {
                grants.push(CapabilityGrant {
                    tool: parsed.tool.clone(),
                    verb: parsed.verb.clone(),
                    resource_pattern: pattern.clone(),
                });
            }
        }

        // Step 3: Build time-bounded manifest.
        let now = Utc::now();
        Ok(CapabilityManifest {
            manifest_id: Uuid::new_v4(),
            agent_id: agent_id.to_string(),
            grants,
            issued_at: now,
            expires_at: now + Duration::hours(options.validity_hours),
        })
    }

    /// Compile with a specific manifest_id (for goal runs that pre-generate IDs).
    pub fn compile_with_id(
        manifest_id: Uuid,
        agent_id: &str,
        profile: &AlignmentProfile,
        options: &CompilerOptions,
    ) -> Result<CapabilityManifest, CompilerError> {
        let mut manifest = Self::compile(agent_id, profile, options)?;
        manifest.manifest_id = manifest_id;
        Ok(manifest)
    }

    /// Validate that no bounded_action overlaps with forbidden_actions.
    fn validate_no_overlaps(profile: &AlignmentProfile) -> Result<(), CompilerError> {
        for bounded in &profile.autonomy_envelope.bounded_actions {
            let parsed = Self::parse_action(bounded)?;
            let canonical = format!("{}_{}", parsed.tool, parsed.verb);

            for forbidden in &profile.autonomy_envelope.forbidden_actions {
                if canonical == *forbidden || *bounded == *forbidden {
                    return Err(CompilerError::ForbiddenOverlap {
                        action: bounded.clone(),
                        message: format!(
                            "'{}' is both bounded and forbidden — this is a contradiction",
                            bounded
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// Parse an action string into a (tool, verb) pair.
    ///
    /// Supported formats:
    /// - `"fs_read"` → tool="fs", verb="read"
    /// - `"fs_write_patch"` → tool="fs", verb="write_patch"
    /// - `"exec: cargo test"` → tool="exec", verb="cargo test"
    /// - `"web_fetch"` → tool="web", verb="fetch"
    fn parse_action(action: &str) -> Result<ParsedAction, CompilerError> {
        // Handle "exec: <command>" format.
        if let Some(command) = action.strip_prefix("exec: ") {
            return Ok(ParsedAction {
                tool: "exec".to_string(),
                verb: command.to_string(),
            });
        }

        // Handle "tool_verb" format — split on first underscore.
        if let Some(underscore_pos) = action.find('_') {
            let tool = &action[..underscore_pos];
            let verb = &action[underscore_pos + 1..];
            if tool.is_empty() || verb.is_empty() {
                return Err(CompilerError::InvalidAction {
                    action: action.to_string(),
                    message: "tool and verb must both be non-empty".to_string(),
                });
            }
            Ok(ParsedAction {
                tool: tool.to_string(),
                verb: verb.to_string(),
            })
        } else {
            Err(CompilerError::InvalidAction {
                action: action.to_string(),
                message: "expected format 'tool_verb' (e.g., 'fs_read') or 'exec: command'"
                    .to_string(),
            })
        }
    }
}

/// A parsed action — intermediate representation.
#[derive(Debug, Clone)]
struct ParsedAction {
    tool: String,
    verb: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alignment::{AlignmentProfile, AutonomyEnvelope, CoordinationConfig};

    fn test_profile() -> AlignmentProfile {
        AlignmentProfile {
            principal: "project-owner".to_string(),
            autonomy_envelope: AutonomyEnvelope {
                bounded_actions: vec![
                    "fs_read".to_string(),
                    "fs_write_patch".to_string(),
                    "fs_apply".to_string(),
                ],
                escalation_triggers: vec!["new_dependency".to_string()],
                forbidden_actions: vec![
                    "network_external".to_string(),
                    "credential_access".to_string(),
                ],
            },
            constitution: "default-v1".to_string(),
            coordination: CoordinationConfig::default(),
        }
    }

    #[test]
    fn compile_default_developer_profile() {
        let profile = AlignmentProfile::default_developer();
        let options = CompilerOptions::default();
        let manifest = PolicyCompiler::compile("claude-code", &profile, &options).unwrap();

        assert_eq!(manifest.agent_id, "claude-code");
        // 3 bounded_actions × 1 resource_scope = 3 grants
        assert_eq!(manifest.grants.len(), 3);
        assert!(!manifest.is_expired());
    }

    #[test]
    fn compile_generates_correct_grants() {
        let profile = test_profile();
        let options = CompilerOptions::default();
        let manifest = PolicyCompiler::compile("agent-1", &profile, &options).unwrap();

        // Should have grants for fs.read, fs.write_patch, fs.apply
        let tools_verbs: Vec<(String, String)> = manifest
            .grants
            .iter()
            .map(|g| (g.tool.clone(), g.verb.clone()))
            .collect();
        assert!(tools_verbs.contains(&("fs".to_string(), "read".to_string())));
        assert!(tools_verbs.contains(&("fs".to_string(), "write_patch".to_string())));
        assert!(tools_verbs.contains(&("fs".to_string(), "apply".to_string())));
    }

    #[test]
    fn compile_applies_resource_scope() {
        let profile = test_profile();
        let options = CompilerOptions {
            resource_scope: vec![
                "fs://workspace/src/**".to_string(),
                "fs://workspace/tests/**".to_string(),
            ],
            validity_hours: 4,
        };
        let manifest = PolicyCompiler::compile("agent-1", &profile, &options).unwrap();

        // 3 actions × 2 scopes = 6 grants
        assert_eq!(manifest.grants.len(), 6);

        // Every grant should match one of the resource scopes
        for grant in &manifest.grants {
            assert!(
                grant.resource_pattern == "fs://workspace/src/**"
                    || grant.resource_pattern == "fs://workspace/tests/**"
            );
        }
    }

    #[test]
    fn compile_with_custom_validity() {
        let profile = test_profile();
        let options = CompilerOptions {
            resource_scope: vec!["fs://workspace/**".to_string()],
            validity_hours: 2,
        };
        let manifest = PolicyCompiler::compile("agent-1", &profile, &options).unwrap();

        let duration = manifest.expires_at - manifest.issued_at;
        assert_eq!(duration.num_hours(), 2);
    }

    #[test]
    fn compile_rejects_forbidden_overlap() {
        let profile = AlignmentProfile {
            principal: "owner".to_string(),
            autonomy_envelope: AutonomyEnvelope {
                bounded_actions: vec!["fs_read".to_string(), "network_external".to_string()],
                escalation_triggers: vec![],
                forbidden_actions: vec!["network_external".to_string()],
            },
            constitution: "default-v1".to_string(),
            coordination: CoordinationConfig::default(),
        };
        let options = CompilerOptions::default();
        let result = PolicyCompiler::compile("agent-1", &profile, &options);
        assert!(result.is_err());
        match result.unwrap_err() {
            CompilerError::ForbiddenOverlap { action, .. } => {
                assert_eq!(action, "network_external");
            }
            other => panic!("expected ForbiddenOverlap, got {:?}", other),
        }
    }

    #[test]
    fn compile_rejects_invalid_action_format() {
        let profile = AlignmentProfile {
            principal: "owner".to_string(),
            autonomy_envelope: AutonomyEnvelope {
                bounded_actions: vec!["invalid".to_string()],
                escalation_triggers: vec![],
                forbidden_actions: vec![],
            },
            constitution: "default-v1".to_string(),
            coordination: CoordinationConfig::default(),
        };
        let options = CompilerOptions::default();
        let result = PolicyCompiler::compile("agent-1", &profile, &options);
        assert!(result.is_err());
        match result.unwrap_err() {
            CompilerError::InvalidAction { action, .. } => {
                assert_eq!(action, "invalid");
            }
            other => panic!("expected InvalidAction, got {:?}", other),
        }
    }

    #[test]
    fn parse_exec_action() {
        let profile = AlignmentProfile {
            principal: "owner".to_string(),
            autonomy_envelope: AutonomyEnvelope {
                bounded_actions: vec!["exec: cargo test".to_string()],
                escalation_triggers: vec![],
                forbidden_actions: vec![],
            },
            constitution: "default-v1".to_string(),
            coordination: CoordinationConfig::default(),
        };
        let options = CompilerOptions::default();
        let manifest = PolicyCompiler::compile("agent-1", &profile, &options).unwrap();

        assert_eq!(manifest.grants.len(), 1);
        assert_eq!(manifest.grants[0].tool, "exec");
        assert_eq!(manifest.grants[0].verb, "cargo test");
    }

    #[test]
    fn compile_with_specific_manifest_id() {
        let profile = test_profile();
        let options = CompilerOptions::default();
        let id = Uuid::new_v4();
        let manifest = PolicyCompiler::compile_with_id(id, "agent-1", &profile, &options).unwrap();
        assert_eq!(manifest.manifest_id, id);
    }

    #[test]
    fn compile_empty_bounded_actions_produces_empty_manifest() {
        let profile = AlignmentProfile {
            principal: "owner".to_string(),
            autonomy_envelope: AutonomyEnvelope {
                bounded_actions: vec![],
                escalation_triggers: vec![],
                forbidden_actions: vec!["fs_write".to_string()],
            },
            constitution: "default-v1".to_string(),
            coordination: CoordinationConfig::default(),
        };
        let options = CompilerOptions::default();
        let manifest = PolicyCompiler::compile("agent-1", &profile, &options).unwrap();
        assert!(manifest.grants.is_empty());
    }

    #[test]
    fn compile_forbidden_actions_not_in_grants() {
        let profile = AlignmentProfile {
            principal: "owner".to_string(),
            autonomy_envelope: AutonomyEnvelope {
                bounded_actions: vec!["fs_read".to_string()],
                escalation_triggers: vec![],
                forbidden_actions: vec!["fs_write".to_string(), "network_external".to_string()],
            },
            constitution: "default-v1".to_string(),
            coordination: CoordinationConfig::default(),
        };
        let options = CompilerOptions::default();
        let manifest = PolicyCompiler::compile("agent-1", &profile, &options).unwrap();

        // Only fs_read should be granted, not fs_write or network_external
        assert_eq!(manifest.grants.len(), 1);
        assert_eq!(manifest.grants[0].tool, "fs");
        assert_eq!(manifest.grants[0].verb, "read");
    }

    #[test]
    fn compiler_error_display() {
        let err = CompilerError::ForbiddenOverlap {
            action: "network_external".to_string(),
            message: "contradicts forbidden list".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("network_external"));
        assert!(display.contains("contradicts"));
    }

    #[test]
    fn compile_multi_segment_verb() {
        // "fs_write_patch" should parse as tool="fs", verb="write_patch"
        let profile = AlignmentProfile {
            principal: "owner".to_string(),
            autonomy_envelope: AutonomyEnvelope {
                bounded_actions: vec!["fs_write_patch".to_string()],
                escalation_triggers: vec![],
                forbidden_actions: vec![],
            },
            constitution: "default-v1".to_string(),
            coordination: CoordinationConfig::default(),
        };
        let options = CompilerOptions::default();
        let manifest = PolicyCompiler::compile("agent-1", &profile, &options).unwrap();
        assert_eq!(manifest.grants[0].tool, "fs");
        assert_eq!(manifest.grants[0].verb, "write_patch");
    }
}
