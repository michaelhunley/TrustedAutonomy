// alignment.rs — Agent Alignment Profile types (v0.4.0).
//
// Alignment profiles extend agent YAML configs with structured declarations
// of an agent's capabilities, constraints, and coordination rules. Unlike
// self-declared alignment cards (AAP), these are *enforced* — the Policy
// Compiler translates them into CapabilityManifest grants.
//
// See PLAN.md v0.4.0 for the full specification.

use serde::{Deserialize, Serialize};

/// An agent's alignment profile — compiled into capability grants by the Policy Compiler.
///
/// This is the structured `alignment` block in an agent's YAML config:
/// ```yaml
/// alignment:
///   principal: "project-owner"
///   autonomy_envelope:
///     bounded_actions: ["fs_read", "fs_write", "exec: cargo test"]
///     escalation_triggers: ["new_dependency", "security_sensitive"]
///     forbidden_actions: ["network_external", "credential_access"]
///   constitution: "default-v1"
///   coordination:
///     allowed_collaborators: ["codex", "claude-flow"]
///     shared_resources: ["src/**", "tests/**"]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlignmentProfile {
    /// Who this agent serves (e.g., "project-owner", "security-team").
    pub principal: String,

    /// The agent's autonomy envelope — what it can, cannot, and must escalate.
    pub autonomy_envelope: AutonomyEnvelope,

    /// Reference to enforcement rules (e.g., "default-v1").
    /// Used by the Policy Compiler to select a constitution template.
    #[serde(default = "default_constitution")]
    pub constitution: String,

    /// Coordination rules for multi-agent scenarios (v0.4.1+).
    #[serde(default)]
    pub coordination: CoordinationConfig,
}

fn default_constitution() -> String {
    "default-v1".to_string()
}

/// The autonomy envelope — defines the boundaries of what an agent can do.
///
/// `bounded_actions` are compiled into CapabilityGrant entries.
/// `forbidden_actions` produce *no* grants (default-deny handles the rest).
/// `escalation_triggers` are compiled into RequireApproval-class grants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutonomyEnvelope {
    /// Actions the agent is allowed to perform.
    /// Format: `"<tool>_<verb>"` (e.g., "fs_read") or `"exec: <command>"`.
    #[serde(default)]
    pub bounded_actions: Vec<String>,

    /// Conditions that force escalation to human approval.
    /// These are semantic labels (e.g., "new_dependency", "security_sensitive").
    #[serde(default)]
    pub escalation_triggers: Vec<String>,

    /// Actions the agent must never perform. Explicitly excluded from grants.
    /// The Policy Compiler validates that no bounded_action overlaps with these.
    #[serde(default)]
    pub forbidden_actions: Vec<String>,
}

/// Coordination rules for multi-agent collaboration (v0.4.1+).
///
/// Used to determine which agents can co-operate on shared resources
/// and which peers this agent is allowed to communicate with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CoordinationConfig {
    /// Agent IDs that this agent is allowed to collaborate with.
    #[serde(default)]
    pub allowed_collaborators: Vec<String>,

    /// Resource patterns (glob) that this agent shares with collaborators.
    #[serde(default)]
    pub shared_resources: Vec<String>,
}

/// An agent setup proposal — the output of the Intent-to-Policy Planner.
///
/// This is the structured plan for how to configure agents for a goal,
/// submitted for human approval before activation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSetupProposal {
    /// Unique ID for this proposal.
    pub proposal_id: String,

    /// The goal this setup is for.
    pub goal_title: String,
    pub goal_objective: String,

    /// Proposed agent configurations.
    pub agents: Vec<ProposedAgent>,

    /// Milestone plan for the goal execution.
    #[serde(default)]
    pub milestones: Vec<Milestone>,

    /// Cost/efficiency notes from the planner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efficiency_notes: Option<String>,
}

/// A proposed agent within an AgentSetupProposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAgent {
    /// The agent ID (must match a known agent config).
    pub agent_id: String,

    /// Role description for this agent in this goal.
    pub role: String,

    /// The alignment profile to use (may override the default from agent YAML).
    pub alignment: AlignmentProfile,

    /// Estimated resource scope (what URIs this agent will access).
    #[serde(default)]
    pub resource_scope: Vec<String>,
}

/// A milestone in the goal execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    /// Milestone description.
    pub description: String,

    /// Which agent is responsible.
    pub agent_id: String,

    /// Expected deliverables.
    #[serde(default)]
    pub deliverables: Vec<String>,
}

impl AlignmentProfile {
    /// Create a default developer alignment profile.
    ///
    /// Grants fs read/write_patch/apply on workspace, denies network and credential access.
    pub fn default_developer() -> Self {
        Self {
            principal: "project-owner".to_string(),
            autonomy_envelope: AutonomyEnvelope {
                bounded_actions: vec![
                    "fs_read".to_string(),
                    "fs_write_patch".to_string(),
                    "fs_apply".to_string(),
                ],
                escalation_triggers: vec![
                    "new_dependency".to_string(),
                    "security_sensitive".to_string(),
                    "breaking_change".to_string(),
                ],
                forbidden_actions: vec![
                    "network_external".to_string(),
                    "credential_access".to_string(),
                ],
            },
            constitution: "default-v1".to_string(),
            coordination: CoordinationConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_profile_yaml_round_trip() {
        let profile = AlignmentProfile::default_developer();
        let yaml = serde_yaml::to_string(&profile).unwrap();
        let restored: AlignmentProfile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(profile, restored);
    }

    #[test]
    fn alignment_profile_json_round_trip() {
        let profile = AlignmentProfile::default_developer();
        let json = serde_json::to_string(&profile).unwrap();
        let restored: AlignmentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, restored);
    }

    #[test]
    fn alignment_profile_yaml_parsing() {
        let yaml = r#"
principal: "security-team"
autonomy_envelope:
  bounded_actions:
    - "fs_read"
  escalation_triggers:
    - "any_write"
  forbidden_actions:
    - "network_external"
    - "credential_access"
    - "fs_write"
constitution: "readonly-v1"
coordination:
  allowed_collaborators:
    - "claude-code"
  shared_resources:
    - "src/**"
"#;
        let profile: AlignmentProfile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(profile.principal, "security-team");
        assert_eq!(profile.autonomy_envelope.bounded_actions.len(), 1);
        assert_eq!(profile.autonomy_envelope.forbidden_actions.len(), 3);
        assert_eq!(profile.constitution, "readonly-v1");
        assert_eq!(profile.coordination.allowed_collaborators.len(), 1);
        assert_eq!(profile.coordination.shared_resources.len(), 1);
    }

    #[test]
    fn alignment_profile_defaults() {
        let yaml = r#"
principal: "owner"
autonomy_envelope:
  bounded_actions: []
"#;
        let profile: AlignmentProfile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(profile.constitution, "default-v1");
        assert!(profile.coordination.allowed_collaborators.is_empty());
        assert!(profile.coordination.shared_resources.is_empty());
        assert!(profile.autonomy_envelope.escalation_triggers.is_empty());
        assert!(profile.autonomy_envelope.forbidden_actions.is_empty());
    }

    #[test]
    fn default_developer_profile_has_expected_actions() {
        let profile = AlignmentProfile::default_developer();
        assert!(profile
            .autonomy_envelope
            .bounded_actions
            .contains(&"fs_read".to_string()));
        assert!(profile
            .autonomy_envelope
            .bounded_actions
            .contains(&"fs_write_patch".to_string()));
        assert!(profile
            .autonomy_envelope
            .forbidden_actions
            .contains(&"network_external".to_string()));
    }

    #[test]
    fn agent_setup_proposal_serialization() {
        let proposal = AgentSetupProposal {
            proposal_id: "prop-001".to_string(),
            goal_title: "Fix auth bug".to_string(),
            goal_objective: "Resolve JWT validation failure".to_string(),
            agents: vec![ProposedAgent {
                agent_id: "claude-code".to_string(),
                role: "Primary coding agent".to_string(),
                alignment: AlignmentProfile::default_developer(),
                resource_scope: vec!["fs://workspace/src/**".to_string()],
            }],
            milestones: vec![Milestone {
                description: "Identify root cause".to_string(),
                agent_id: "claude-code".to_string(),
                deliverables: vec!["diagnosis.md".to_string()],
            }],
            efficiency_notes: Some("Single agent sufficient for this scope".to_string()),
        };

        let json = serde_json::to_string_pretty(&proposal).unwrap();
        let restored: AgentSetupProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.proposal_id, "prop-001");
        assert_eq!(restored.agents.len(), 1);
        assert_eq!(restored.milestones.len(), 1);
    }

    #[test]
    fn proposed_agent_inherits_alignment() {
        let agent = ProposedAgent {
            agent_id: "claude-code".to_string(),
            role: "Coder".to_string(),
            alignment: AlignmentProfile {
                principal: "team-lead".to_string(),
                autonomy_envelope: AutonomyEnvelope {
                    bounded_actions: vec!["fs_read".to_string(), "fs_write".to_string()],
                    escalation_triggers: vec![],
                    forbidden_actions: vec!["credential_access".to_string()],
                },
                constitution: "scoped-v1".to_string(),
                coordination: CoordinationConfig {
                    allowed_collaborators: vec!["codex".to_string()],
                    shared_resources: vec!["src/**".to_string()],
                },
            },
            resource_scope: vec!["fs://workspace/src/**".to_string()],
        };

        let json = serde_json::to_string(&agent).unwrap();
        let restored: ProposedAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.alignment.principal, "team-lead");
        assert_eq!(restored.alignment.constitution, "scoped-v1");
    }

    #[test]
    fn coordination_config_default_is_empty() {
        let config = CoordinationConfig::default();
        assert!(config.allowed_collaborators.is_empty());
        assert!(config.shared_resources.is_empty());
    }
}
