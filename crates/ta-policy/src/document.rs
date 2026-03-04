// document.rs — PolicyDocument: the unified policy configuration surface.
//
// All supervision configuration resolves to a single PolicyDocument. This is
// the merged result of the 6-layer policy cascade (see cascade.rs).
//
// Users configure policy via `.ta/policy.yaml`. The cascade merges built-in
// defaults, project config, workflow overrides, agent profiles, goal
// constitutions, and CLI flags into one document.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The unified policy document — the merged result of all policy layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDocument {
    /// Schema version for forward compatibility.
    #[serde(default = "default_version")]
    pub version: String,

    /// Global defaults applied to all agents and goals.
    #[serde(default)]
    pub defaults: PolicyDefaults,

    /// Per-URI-scheme policy rules (e.g., fs, email, db, api).
    #[serde(default)]
    pub schemes: HashMap<String, SchemePolicy>,

    /// Escalation triggers — conditions that force human review.
    #[serde(default)]
    pub escalation: EscalationConfig,

    /// Per-agent policy overrides.
    #[serde(default)]
    pub agents: HashMap<String, AgentPolicyOverride>,

    /// Security level controls review stringency.
    #[serde(default)]
    pub security_level: SecurityLevel,

    /// Optional budget limits.
    #[serde(default)]
    pub budget: Option<BudgetConfig>,
}

fn default_version() -> String {
    "1".to_string()
}

impl Default for PolicyDocument {
    fn default() -> Self {
        Self {
            version: default_version(),
            defaults: PolicyDefaults::default(),
            schemes: HashMap::new(),
            escalation: EscalationConfig::default(),
            agents: HashMap::new(),
            security_level: SecurityLevel::default(),
            budget: None,
        }
    }
}

/// Global policy defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyDefaults {
    /// How strict enforcement is at the project level.
    #[serde(default)]
    pub enforcement: PolicyEnforcement,

    /// What can be auto-approved without human review.
    #[serde(default)]
    pub auto_approve: AutoApproveConfig,
}

/// Enforcement strictness (project-level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEnforcement {
    /// Log policy decisions but don't block.
    Warning,
    /// Block on policy violations (default).
    #[default]
    Error,
    /// Block and require constitutions for every goal.
    Strict,
}

impl std::fmt::Display for PolicyEnforcement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyEnforcement::Warning => write!(f, "warning"),
            PolicyEnforcement::Error => write!(f, "error"),
            PolicyEnforcement::Strict => write!(f, "strict"),
        }
    }
}

/// Auto-approval configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoApproveConfig {
    /// Auto-approve read-only actions (no state change).
    #[serde(default = "default_true")]
    pub read_only: bool,

    /// Auto-approve internal TA tool calls (ta_* MCP tools).
    #[serde(default = "default_true")]
    pub internal_tools: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AutoApproveConfig {
    fn default() -> Self {
        Self {
            read_only: true,
            internal_tools: true,
        }
    }
}

/// Per-URI-scheme policy rules.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemePolicy {
    /// Verbs that require human approval for this scheme.
    #[serde(default)]
    pub approval_required: Vec<String>,

    /// Whether this scheme requires a credential to be configured.
    #[serde(default)]
    pub credential_required: bool,

    /// Max actions per session (None = unlimited).
    #[serde(default)]
    pub max_actions_per_session: Option<u32>,
}

/// Escalation triggers — conditions that force human review.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EscalationConfig {
    /// Trigger escalation when drift score exceeds this threshold (0.0-1.0).
    #[serde(default)]
    pub drift_threshold: Option<f64>,

    /// Trigger escalation when action count exceeds this limit.
    #[serde(default)]
    pub action_count_limit: Option<u32>,

    /// Custom escalation trigger patterns.
    #[serde(default)]
    pub patterns: Vec<String>,
}

/// Per-agent policy overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentPolicyOverride {
    /// Additional verbs that require approval for this agent.
    #[serde(default)]
    pub additional_approval_required: Vec<String>,

    /// Actions this agent is explicitly forbidden from.
    #[serde(default)]
    pub forbidden_actions: Vec<String>,

    /// Override the security level for this agent.
    #[serde(default)]
    pub security_level: Option<SecurityLevel>,
}

/// Security level — controls how strictly TA mediates actions.
///
/// Ordered from least to most restrictive. The cascade can only increase
/// the security level, never decrease it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum SecurityLevel {
    /// Audit-only: log everything, block nothing.
    Open,
    /// Review at draft boundaries (default).
    #[default]
    Checkpoint,
    /// Approve each state-changing action individually.
    Supervised,
    /// Constitutions required; every action evaluated against declared intent.
    Strict,
}

impl std::fmt::Display for SecurityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityLevel::Open => write!(f, "open"),
            SecurityLevel::Checkpoint => write!(f, "checkpoint"),
            SecurityLevel::Supervised => write!(f, "supervised"),
            SecurityLevel::Strict => write!(f, "strict"),
        }
    }
}

/// Budget configuration — limits on resource usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Maximum tokens per goal (None = unlimited).
    #[serde(default)]
    pub max_tokens_per_goal: Option<u64>,

    /// Warn agent when this percentage of budget is spent.
    #[serde(default = "default_warn_percent")]
    pub warn_at_percent: u8,
}

fn default_warn_percent() -> u8 {
    80
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_tokens_per_goal: None,
            warn_at_percent: 80,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_document_is_checkpoint() {
        let doc = PolicyDocument::default();
        assert_eq!(doc.security_level, SecurityLevel::Checkpoint);
        assert_eq!(doc.defaults.enforcement, PolicyEnforcement::Error);
        assert!(doc.defaults.auto_approve.read_only);
    }

    #[test]
    fn yaml_round_trip() {
        let yaml = r#"
version: "1"
defaults:
  enforcement: strict
  auto_approve:
    read_only: true
    internal_tools: false
schemes:
  fs:
    approval_required: [apply, delete]
  email:
    approval_required: [send, delete]
    credential_required: true
    max_actions_per_session: 10
escalation:
  drift_threshold: 0.7
  action_count_limit: 100
  patterns:
    - new_dependency
    - config_change
security_level: supervised
budget:
  max_tokens_per_goal: 500000
  warn_at_percent: 80
"#;
        let doc: PolicyDocument = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(doc.defaults.enforcement, PolicyEnforcement::Strict);
        assert!(!doc.defaults.auto_approve.internal_tools);
        assert_eq!(doc.schemes.len(), 2);
        assert_eq!(doc.schemes["fs"].approval_required, vec!["apply", "delete"]);
        assert!(doc.schemes["email"].credential_required);
        assert_eq!(doc.schemes["email"].max_actions_per_session, Some(10));
        assert_eq!(doc.escalation.drift_threshold, Some(0.7));
        assert_eq!(doc.escalation.action_count_limit, Some(100));
        assert_eq!(doc.security_level, SecurityLevel::Supervised);
        assert_eq!(
            doc.budget.as_ref().unwrap().max_tokens_per_goal,
            Some(500000)
        );

        // Re-serialize and parse again to verify round-trip.
        let yaml_out = serde_yaml::to_string(&doc).unwrap();
        let doc2: PolicyDocument = serde_yaml::from_str(&yaml_out).unwrap();
        assert_eq!(doc2.security_level, doc.security_level);
    }

    #[test]
    fn security_level_ordering() {
        assert!(SecurityLevel::Open < SecurityLevel::Checkpoint);
        assert!(SecurityLevel::Checkpoint < SecurityLevel::Supervised);
        assert!(SecurityLevel::Supervised < SecurityLevel::Strict);
    }

    #[test]
    fn enforcement_ordering() {
        assert!(PolicyEnforcement::Warning < PolicyEnforcement::Error);
        assert!(PolicyEnforcement::Error < PolicyEnforcement::Strict);
    }

    #[test]
    fn agent_override_deserialization() {
        let yaml = r#"
additional_approval_required:
  - write
forbidden_actions:
  - network_external
security_level: strict
"#;
        let override_: AgentPolicyOverride = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(override_.additional_approval_required, vec!["write"]);
        assert_eq!(override_.forbidden_actions, vec!["network_external"]);
        assert_eq!(override_.security_level, Some(SecurityLevel::Strict));
    }

    #[test]
    fn budget_defaults() {
        let budget = BudgetConfig::default();
        assert!(budget.max_tokens_per_goal.is_none());
        assert_eq!(budget.warn_at_percent, 80);
    }

    #[test]
    fn security_level_display() {
        assert_eq!(format!("{}", SecurityLevel::Open), "open");
        assert_eq!(format!("{}", SecurityLevel::Checkpoint), "checkpoint");
        assert_eq!(format!("{}", SecurityLevel::Supervised), "supervised");
        assert_eq!(format!("{}", SecurityLevel::Strict), "strict");
    }

    #[test]
    fn minimal_yaml_parses_with_defaults() {
        let yaml = "{}";
        let doc: PolicyDocument = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(doc.version, "1");
        assert_eq!(doc.security_level, SecurityLevel::Checkpoint);
        assert!(doc.schemes.is_empty());
    }
}
