// cascade.rs — PolicyCascade: loads and merges policy from multiple layers.
//
// The 6-layer policy cascade (each layer tightens, never loosens):
//   1. Built-in defaults (hardcoded)
//   2. .ta/policy.yaml (project config)
//   3. .ta/workflows/<name>.yaml (workflow overrides)
//   4. agents/<name>.yaml (agent profile)
//   5. .ta/constitutions/goal-<id>.yaml (goal constitution)
//   6. CLI overrides (flags)
//
// The merge rule: each layer can add restrictions (verbs to approval_required,
// increase security_level, add escalation triggers) — never remove them.

use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::document::{PolicyDocument, SecurityLevel};
use crate::error::PolicyError;

/// CLI-level overrides passed as flags.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    /// Override the security level (e.g., --strict, --open).
    pub security_level: Option<SecurityLevel>,
    /// Force auto-approve mode (e.g., --auto-approve).
    pub auto_approve: Option<bool>,
}

/// Loads and merges policy from all 6 cascade layers.
pub struct PolicyCascade;

impl PolicyCascade {
    /// Load the fully-merged policy document for a given context.
    ///
    /// - `project_root`: path to the project (containing `.ta/`)
    /// - `agent_id`: which agent's profile to apply
    /// - `goal_id`: which goal's constitution to apply (if any)
    /// - `workflow`: which workflow overrides to apply (if any)
    /// - `overrides`: CLI-level flags
    pub fn load(
        project_root: &Path,
        agent_id: &str,
        goal_id: Option<Uuid>,
        workflow: Option<&str>,
        overrides: &CliOverrides,
    ) -> Result<PolicyDocument, PolicyError> {
        // Layer 1: Built-in defaults.
        let mut doc = PolicyDocument::default();

        // Layer 2: Project policy (.ta/policy.yaml).
        let project_policy_path = project_root.join(".ta/policy.yaml");
        if project_policy_path.exists() {
            let project_doc = Self::load_yaml(&project_policy_path)?;
            Self::merge(&mut doc, &project_doc);
        }

        // Layer 3: Workflow policy (.ta/workflows/<name>.yaml).
        if let Some(wf) = workflow {
            let workflow_path = project_root.join(format!(".ta/workflows/{}.yaml", wf));
            if workflow_path.exists() {
                let wf_doc = Self::load_yaml(&workflow_path)?;
                Self::merge(&mut doc, &wf_doc);
            }
        }

        // Layer 4: Agent profile (agents/<name>.yaml → policy section).
        let agent_policy_path = project_root.join(format!(".ta/agents/{}.policy.yaml", agent_id));
        if agent_policy_path.exists() {
            let agent_doc = Self::load_yaml(&agent_policy_path)?;
            Self::merge(&mut doc, &agent_doc);
        }

        // Layer 5: Goal constitution (.ta/constitutions/goal-<id>.yaml).
        if let Some(gid) = goal_id {
            let constitution_path =
                project_root.join(format!(".ta/constitutions/goal-{}.yaml", gid));
            if constitution_path.exists() {
                let const_doc = Self::load_yaml(&constitution_path)?;
                Self::merge(&mut doc, &const_doc);
            }
        }

        // Layer 6: CLI overrides.
        if let Some(level) = overrides.security_level {
            if level > doc.security_level {
                doc.security_level = level;
            }
        }
        if let Some(auto) = overrides.auto_approve {
            if !auto {
                doc.defaults.auto_approve.read_only = false;
                doc.defaults.auto_approve.internal_tools = false;
            }
        }

        Ok(doc)
    }

    /// Load a PolicyDocument from a YAML file.
    fn load_yaml(path: &Path) -> Result<PolicyDocument, PolicyError> {
        let content = fs::read_to_string(path).map_err(|e| PolicyError::IoError {
            path: path.display().to_string(),
            source: e,
        })?;
        let doc: PolicyDocument = serde_yaml::from_str(&content).map_err(|e| {
            PolicyError::ConfigError(format!("invalid policy YAML at {}: {}", path.display(), e))
        })?;
        Ok(doc)
    }

    /// Merge an overlay document onto a base document (tighten-only).
    ///
    /// The overlay can add restrictions but never remove them:
    /// - Security level: only increase (Open < Checkpoint < Supervised < Strict)
    /// - Enforcement: only increase (Warning < Error < Strict)
    /// - Scheme policies: merge approval_required lists (union, not replace)
    /// - Escalation: add patterns, lower thresholds
    /// - Agent overrides: merge additively
    fn merge(base: &mut PolicyDocument, overlay: &PolicyDocument) {
        // Security level: only tighten.
        if overlay.security_level > base.security_level {
            base.security_level = overlay.security_level;
        }

        // Enforcement: only tighten.
        if overlay.defaults.enforcement > base.defaults.enforcement {
            base.defaults.enforcement = overlay.defaults.enforcement;
        }

        // Auto-approve: overlay can disable but not enable.
        if !overlay.defaults.auto_approve.read_only {
            base.defaults.auto_approve.read_only = false;
        }
        if !overlay.defaults.auto_approve.internal_tools {
            base.defaults.auto_approve.internal_tools = false;
        }

        // Scheme policies: merge additively.
        for (scheme, overlay_policy) in &overlay.schemes {
            let base_policy = base.schemes.entry(scheme.clone()).or_default();

            // Add approval verbs (union).
            for verb in &overlay_policy.approval_required {
                if !base_policy.approval_required.contains(verb) {
                    base_policy.approval_required.push(verb.clone());
                }
            }

            // Credential requirement can only be tightened (false → true).
            if overlay_policy.credential_required {
                base_policy.credential_required = true;
            }

            // Action limit: take the lower value.
            match (
                base_policy.max_actions_per_session,
                overlay_policy.max_actions_per_session,
            ) {
                (None, Some(v)) => base_policy.max_actions_per_session = Some(v),
                (Some(base_v), Some(overlay_v)) if overlay_v < base_v => {
                    base_policy.max_actions_per_session = Some(overlay_v);
                }
                _ => {}
            }
        }

        // Escalation: tighten thresholds, add patterns.
        if let Some(overlay_thresh) = overlay.escalation.drift_threshold {
            match base.escalation.drift_threshold {
                None => base.escalation.drift_threshold = Some(overlay_thresh),
                Some(base_thresh) if overlay_thresh < base_thresh => {
                    base.escalation.drift_threshold = Some(overlay_thresh);
                }
                _ => {}
            }
        }
        if let Some(overlay_limit) = overlay.escalation.action_count_limit {
            match base.escalation.action_count_limit {
                None => base.escalation.action_count_limit = Some(overlay_limit),
                Some(base_limit) if overlay_limit < base_limit => {
                    base.escalation.action_count_limit = Some(overlay_limit);
                }
                _ => {}
            }
        }
        for pattern in &overlay.escalation.patterns {
            if !base.escalation.patterns.contains(pattern) {
                base.escalation.patterns.push(pattern.clone());
            }
        }

        // Agent overrides: merge additively.
        for (agent, overlay_override) in &overlay.agents {
            let base_override = base.agents.entry(agent.clone()).or_default();

            for verb in &overlay_override.additional_approval_required {
                if !base_override.additional_approval_required.contains(verb) {
                    base_override
                        .additional_approval_required
                        .push(verb.clone());
                }
            }
            for action in &overlay_override.forbidden_actions {
                if !base_override.forbidden_actions.contains(action) {
                    base_override.forbidden_actions.push(action.clone());
                }
            }
            if let Some(level) = overlay_override.security_level {
                match base_override.security_level {
                    None => base_override.security_level = Some(level),
                    Some(base_level) if level > base_level => {
                        base_override.security_level = Some(level);
                    }
                    _ => {}
                }
            }
        }

        // Budget: take the lower limit.
        if let Some(overlay_budget) = &overlay.budget {
            match &mut base.budget {
                None => base.budget = Some(overlay_budget.clone()),
                Some(base_budget) => {
                    match (
                        base_budget.max_tokens_per_goal,
                        overlay_budget.max_tokens_per_goal,
                    ) {
                        (None, Some(v)) => base_budget.max_tokens_per_goal = Some(v),
                        (Some(bv), Some(ov)) if ov < bv => {
                            base_budget.max_tokens_per_goal = Some(ov);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::document::{BudgetConfig, SchemePolicy};
    use tempfile::TempDir;

    #[test]
    fn load_defaults_without_config_file() {
        let temp = TempDir::new().unwrap();
        let doc = PolicyCascade::load(temp.path(), "agent-1", None, None, &CliOverrides::default())
            .unwrap();

        assert_eq!(doc.security_level, SecurityLevel::Checkpoint);
        assert!(doc.defaults.auto_approve.read_only);
    }

    #[test]
    fn load_from_project_policy_yaml() {
        let temp = TempDir::new().unwrap();
        let ta_dir = temp.path().join(".ta");
        fs::create_dir_all(&ta_dir).unwrap();

        let yaml = r#"
security_level: supervised
schemes:
  fs:
    approval_required: [apply, delete]
  email:
    approval_required: [send]
    credential_required: true
"#;
        fs::write(ta_dir.join("policy.yaml"), yaml).unwrap();

        let doc = PolicyCascade::load(temp.path(), "agent-1", None, None, &CliOverrides::default())
            .unwrap();

        assert_eq!(doc.security_level, SecurityLevel::Supervised);
        assert_eq!(doc.schemes["fs"].approval_required, vec!["apply", "delete"]);
        assert!(doc.schemes["email"].credential_required);
    }

    #[test]
    fn merge_tightens_security_level() {
        let mut base = PolicyDocument::default(); // Checkpoint
        let overlay = PolicyDocument {
            security_level: SecurityLevel::Supervised,
            ..Default::default()
        };

        PolicyCascade::merge(&mut base, &overlay);
        assert_eq!(base.security_level, SecurityLevel::Supervised);
    }

    #[test]
    fn merge_cannot_loosen_security_level() {
        let mut base = PolicyDocument {
            security_level: SecurityLevel::Supervised,
            ..Default::default()
        };
        let overlay = PolicyDocument {
            security_level: SecurityLevel::Open,
            ..Default::default()
        };

        PolicyCascade::merge(&mut base, &overlay);
        assert_eq!(base.security_level, SecurityLevel::Supervised); // unchanged
    }

    #[test]
    fn merge_unions_approval_verbs() {
        let mut base = PolicyDocument::default();
        base.schemes.insert(
            "fs".to_string(),
            SchemePolicy {
                approval_required: vec!["apply".to_string()],
                ..Default::default()
            },
        );

        let mut overlay = PolicyDocument::default();
        overlay.schemes.insert(
            "fs".to_string(),
            SchemePolicy {
                approval_required: vec!["apply".to_string(), "delete".to_string()],
                ..Default::default()
            },
        );

        PolicyCascade::merge(&mut base, &overlay);
        let fs_policy = &base.schemes["fs"];
        assert!(fs_policy.approval_required.contains(&"apply".to_string()));
        assert!(fs_policy.approval_required.contains(&"delete".to_string()));
        assert_eq!(fs_policy.approval_required.len(), 2); // no duplicates
    }

    #[test]
    fn merge_takes_lower_action_limit() {
        let mut base = PolicyDocument::default();
        base.schemes.insert(
            "email".to_string(),
            SchemePolicy {
                max_actions_per_session: Some(100),
                ..Default::default()
            },
        );

        let mut overlay = PolicyDocument::default();
        overlay.schemes.insert(
            "email".to_string(),
            SchemePolicy {
                max_actions_per_session: Some(50),
                ..Default::default()
            },
        );

        PolicyCascade::merge(&mut base, &overlay);
        assert_eq!(base.schemes["email"].max_actions_per_session, Some(50));
    }

    #[test]
    fn merge_adds_escalation_patterns() {
        let mut base = PolicyDocument::default();
        base.escalation.patterns.push("new_dependency".to_string());

        let mut overlay = PolicyDocument::default();
        overlay
            .escalation
            .patterns
            .push("config_change".to_string());
        overlay
            .escalation
            .patterns
            .push("new_dependency".to_string()); // duplicate

        PolicyCascade::merge(&mut base, &overlay);
        assert_eq!(base.escalation.patterns.len(), 2);
    }

    #[test]
    fn merge_lowers_drift_threshold() {
        let mut base = PolicyDocument::default();
        base.escalation.drift_threshold = Some(0.8);

        let mut overlay = PolicyDocument::default();
        overlay.escalation.drift_threshold = Some(0.5);

        PolicyCascade::merge(&mut base, &overlay);
        assert_eq!(base.escalation.drift_threshold, Some(0.5));
    }

    #[test]
    fn cli_overrides_tighten_only() {
        let temp = TempDir::new().unwrap();

        // CLI raises security to Strict.
        let overrides = CliOverrides {
            security_level: Some(SecurityLevel::Strict),
            auto_approve: None,
        };

        let doc = PolicyCascade::load(temp.path(), "agent", None, None, &overrides).unwrap();
        assert_eq!(doc.security_level, SecurityLevel::Strict);
    }

    #[test]
    fn cli_override_cannot_lower_project_level() {
        let temp = TempDir::new().unwrap();
        let ta_dir = temp.path().join(".ta");
        fs::create_dir_all(&ta_dir).unwrap();
        fs::write(ta_dir.join("policy.yaml"), "security_level: supervised\n").unwrap();

        // CLI tries to set Open (lower than Supervised).
        let overrides = CliOverrides {
            security_level: Some(SecurityLevel::Open),
            auto_approve: None,
        };

        let doc = PolicyCascade::load(temp.path(), "agent", None, None, &overrides).unwrap();
        assert_eq!(doc.security_level, SecurityLevel::Supervised); // stays supervised
    }

    #[test]
    fn merge_budget_takes_lower() {
        let mut base = PolicyDocument::default();
        base.budget = Some(BudgetConfig {
            max_tokens_per_goal: Some(1_000_000),
            warn_at_percent: 80,
        });

        let mut overlay = PolicyDocument::default();
        overlay.budget = Some(BudgetConfig {
            max_tokens_per_goal: Some(500_000),
            warn_at_percent: 80,
        });

        PolicyCascade::merge(&mut base, &overlay);
        assert_eq!(
            base.budget.as_ref().unwrap().max_tokens_per_goal,
            Some(500_000)
        );
    }
}
