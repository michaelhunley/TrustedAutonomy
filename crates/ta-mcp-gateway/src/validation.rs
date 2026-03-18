// validation.rs — Shared helpers for MCP tool handlers.

use rmcp::ErrorData as McpError;
use uuid::Uuid;

use ta_goal::{GoalRun, GoalRunStore};
use ta_policy::PolicyDecision;

/// Parse a UUID string, returning an MCP error on failure.
pub fn parse_uuid(s: &str) -> Result<Uuid, McpError> {
    Uuid::parse_str(s)
        .map_err(|e| McpError::invalid_params(format!("invalid UUID '{}': {}", s, e), None))
}

/// Validate that a goal_run_id corresponds to an existing goal in the store.
/// Returns the goal on success, or an MCP invalid_params error if not found.
pub fn validate_goal_exists(
    goal_store: &GoalRunStore,
    goal_run_id: Uuid,
) -> Result<GoalRun, McpError> {
    goal_store
        .get(goal_run_id)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?
        .ok_or_else(|| {
            McpError::invalid_params(format!("goal_run_id not found: {}", goal_run_id), None)
        })
}

/// Enforce a policy decision, returning an MCP error if denied.
pub fn enforce_policy(decision: &PolicyDecision) -> Result<(), McpError> {
    match decision {
        PolicyDecision::Allow => Ok(()),
        PolicyDecision::Deny { reason } => Err(McpError::invalid_request(
            format!("Policy denied: {}", reason),
            None,
        )),
        PolicyDecision::RequireApproval { reason } => {
            // For now, RequireApproval is treated as allowed since the
            // CLI approval flow will handle the actual gating.
            tracing::info!("action requires approval: {}", reason);
            Ok(())
        }
    }
}

// §7 regression tests: enforce_policy must deny access when policy denies.
// These tests ensure that any future refactor of enforce_policy() cannot
// accidentally allow a Deny decision through.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforce_policy_allow_passes() {
        // §7: Allow decision should succeed.
        assert!(enforce_policy(&PolicyDecision::Allow).is_ok());
    }

    #[test]
    fn enforce_policy_deny_returns_error() {
        // §7: Deny decision MUST return an error — prevents policy bypass.
        let result = enforce_policy(&PolicyDecision::Deny {
            reason: "agent lacks read grant for this path".to_string(),
        });
        assert!(result.is_err(), "Deny decision must produce an MCP error");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Policy denied"),
            "error message must include 'Policy denied', got: {}",
            err.message
        );
    }

    #[test]
    fn enforce_policy_require_approval_passes() {
        // §7: RequireApproval is gated at the CLI review flow, not here.
        let result = enforce_policy(&PolicyDecision::RequireApproval {
            reason: "apply verb requires explicit approval".to_string(),
        });
        assert!(result.is_ok());
    }

    #[test]
    fn parse_uuid_valid() {
        let id = uuid::Uuid::new_v4();
        let parsed = parse_uuid(&id.to_string());
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap(), id);
    }

    #[test]
    fn parse_uuid_invalid_returns_error() {
        let result = parse_uuid("not-a-uuid");
        assert!(result.is_err());
    }
}
