// vertical_slice.rs — End-to-end integration test proving the core thesis.
//
// This single test exercises the complete Trusted Autonomy flow:
//
//   1. Set up policy engine with capability manifest (default deny)
//   2. Policy check: fs.read → Allow
//   3. Policy check: fs.write_patch → Allow
//   4. Agent reads a source file through the connector
//   5. Agent writes a modified file through the connector → ChangeSet created
//   6. Agent writes a new file through the connector → ChangeSet created
//   7. All operations logged to audit trail
//   8. Build PR package → bundles all changes for review
//   9. Policy check: fs.apply → RequireApproval (always gated!)
//  10. Simulate human approval → apply changes to target directory
//  11. Log apply event to audit
//
// VERIFY:
//   - Files exist in target directory with correct content
//   - Changesets persisted in the store
//   - PR package contains correct artifacts and diffs
//   - Audit log contains all events with intact hash chain
//   - Policy decisions are correct at every step
//
// This proves the thesis: "agent writes file → staged as changeset →
// PR package built → approved → applied to real filesystem" — all
// mediated through policy and audit.

use std::fs;

use chrono::{Duration, Utc};
use tempfile::tempdir;
use uuid::Uuid;

use ta_audit::{AuditAction, AuditLog};
use ta_changeset::pr_package::PRStatus;
use ta_changeset::DiffContent;
use ta_connector_fs::FsConnector;
use ta_policy::{CapabilityGrant, CapabilityManifest, PolicyDecision, PolicyEngine, PolicyRequest};
use ta_workspace::{JsonFileStore, StagingWorkspace};

/// The complete vertical slice integration test.
///
/// This is the most important test in the entire project. It proves that
/// every layer (audit, policy, changeset, workspace, connector) works
/// together to mediate agent actions safely.
#[test]
fn full_vertical_slice_agent_to_apply() {
    // =========================================================
    // SETUP: Create all the infrastructure
    // =========================================================

    // Temp directories for everything.
    let source_dir = tempdir().unwrap(); // Simulates the "real" filesystem
    let target_dir = tempdir().unwrap(); // Where approved changes get applied
    let staging_root = tempdir().unwrap(); // Staging workspace
    let store_dir = tempdir().unwrap(); // ChangeSet persistence
    let audit_dir = tempdir().unwrap(); // Audit log

    // Create a "source" file to simulate an existing codebase.
    let source_file = source_dir.path().join("config.toml");
    fs::write(
        &source_file,
        b"[server]\nport = 8080\nhost = \"localhost\"\n",
    )
    .unwrap();

    // Set up the audit log.
    let audit_path = audit_dir.path().join("audit.jsonl");
    let audit_log = AuditLog::open(&audit_path).unwrap();

    // Set up the staging workspace and change store.
    let staging = StagingWorkspace::new("goal-1", staging_root.path()).unwrap();
    let store = JsonFileStore::new(store_dir.path().join("store")).unwrap();

    // Create the filesystem connector with audit logging.
    let mut connector =
        FsConnector::new("goal-1", staging, store, "agent-1").with_audit_log(audit_log);

    // =========================================================
    // STEP 1: Set up policy engine with agent's manifest
    // =========================================================

    let mut policy_engine = PolicyEngine::new();

    // Grant the agent permission to read and write_patch (but not apply).
    let manifest = CapabilityManifest {
        manifest_id: Uuid::new_v4(),
        agent_id: "agent-1".to_string(),
        grants: vec![
            CapabilityGrant {
                tool: "fs".to_string(),
                verb: "read".to_string(),
                resource_pattern: "fs://source/**".to_string(),
            },
            CapabilityGrant {
                tool: "fs".to_string(),
                verb: "write_patch".to_string(),
                resource_pattern: "fs://workspace/**".to_string(),
            },
            // Note: we also grant "apply" so the policy engine returns
            // RequireApproval (not Deny). The point is that apply always
            // needs approval even when granted.
            CapabilityGrant {
                tool: "fs".to_string(),
                verb: "apply".to_string(),
                resource_pattern: "fs://target/**".to_string(),
            },
        ],
        issued_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(1),
    };

    policy_engine.load_manifest(manifest);

    // =========================================================
    // STEP 2: Policy check — fs.read → Allow
    // =========================================================

    let read_decision = policy_engine.evaluate(&PolicyRequest {
        agent_id: "agent-1".to_string(),
        tool: "fs".to_string(),
        verb: "read".to_string(),
        target_uri: "fs://source/config.toml".to_string(),
    });
    assert_eq!(read_decision, PolicyDecision::Allow);

    // =========================================================
    // STEP 3: Policy check — fs.write_patch → Allow
    // =========================================================

    let write_decision = policy_engine.evaluate(&PolicyRequest {
        agent_id: "agent-1".to_string(),
        tool: "fs".to_string(),
        verb: "write_patch".to_string(),
        target_uri: "fs://workspace/config.toml".to_string(),
    });
    assert_eq!(write_decision, PolicyDecision::Allow);

    // =========================================================
    // STEP 4: Agent reads the source file (snapshots original)
    // =========================================================

    let original_content = connector
        .read_source(source_dir.path(), "config.toml")
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&original_content),
        "[server]\nport = 8080\nhost = \"localhost\"\n"
    );

    // =========================================================
    // STEP 5: Agent writes a MODIFIED file → ChangeSet #1
    // =========================================================

    let modified_content = b"[server]\nport = 9090\nhost = \"0.0.0.0\"\n";
    let changeset_1 = connector
        .write_patch("config.toml", modified_content)
        .unwrap();

    // Verify changeset has correct metadata.
    assert_eq!(changeset_1.target_uri, "fs://workspace/config.toml");
    assert!(changeset_1.verify_hash()); // Integrity check

    // Verify the diff shows the actual change.
    match &changeset_1.diff_content {
        DiffContent::UnifiedDiff { content } => {
            assert!(
                content.contains("-port = 8080"),
                "diff should show old port"
            );
            assert!(
                content.contains("+port = 9090"),
                "diff should show new port"
            );
        }
        other => panic!("expected UnifiedDiff for modified file, got {:?}", other),
    }

    // =========================================================
    // STEP 6: Agent writes a NEW file → ChangeSet #2
    // =========================================================

    let new_file_content = b"# Deployment Notes\n\nUpdated port to 9090 for external access.\n";
    let changeset_2 = connector
        .write_patch("DEPLOY.md", new_file_content)
        .unwrap();

    assert_eq!(changeset_2.target_uri, "fs://workspace/DEPLOY.md");
    match &changeset_2.diff_content {
        DiffContent::CreateFile { content } => {
            assert!(content.contains("Deployment Notes"));
        }
        other => panic!("expected CreateFile for new file, got {:?}", other),
    }

    // =========================================================
    // STEP 7: Verify changesets are persisted in the store
    // =========================================================

    let all_changesets = connector.list_changesets().unwrap();
    assert_eq!(all_changesets.len(), 2, "should have exactly 2 changesets");
    assert_eq!(all_changesets[0].changeset_id, changeset_1.changeset_id);
    assert_eq!(all_changesets[1].changeset_id, changeset_2.changeset_id);

    // =========================================================
    // STEP 8: Build PR package → bundles all changes for review
    // =========================================================

    let pr_package = connector
        .build_pr_package(
            "Update Server Config",
            "Change port and host for external access",
            "Modified config.toml (port 8080→9090, host→0.0.0.0) and added deployment notes",
            "Preparing for external deployment",
        )
        .unwrap();

    // Verify PR package structure.
    assert_eq!(pr_package.goal.goal_id, "goal-1");
    assert_eq!(pr_package.goal.title, "Update Server Config");
    assert_eq!(pr_package.changes.artifacts.len(), 2);
    assert_eq!(pr_package.status, PRStatus::PendingReview);

    // Verify both artifacts are in the package.
    let artifact_uris: Vec<&str> = pr_package
        .changes
        .artifacts
        .iter()
        .map(|a| a.resource_uri.as_str())
        .collect();
    assert!(artifact_uris.contains(&"fs://workspace/config.toml"));
    assert!(artifact_uris.contains(&"fs://workspace/DEPLOY.md"));

    // Verify the package can serialize to JSON (the schema format).
    let package_json = serde_json::to_string_pretty(&pr_package).unwrap();
    assert!(package_json.contains("\"package_version\""));
    assert!(package_json.contains("\"goal\""));
    assert!(package_json.contains("\"changes\""));
    assert!(package_json.contains("\"risk\""));
    assert!(package_json.contains("\"review_requests\""));

    // =========================================================
    // STEP 9: Policy check — fs.apply → RequireApproval
    // =========================================================

    let apply_decision = policy_engine.evaluate(&PolicyRequest {
        agent_id: "agent-1".to_string(),
        tool: "fs".to_string(),
        verb: "apply".to_string(),
        target_uri: "fs://target/config.toml".to_string(),
    });

    // Even though the agent has a grant for apply, the policy engine
    // ALWAYS returns RequireApproval for side-effecting verbs.
    // This is the "human-in-the-loop" guarantee.
    assert!(
        matches!(apply_decision, PolicyDecision::RequireApproval { .. }),
        "apply must always require approval, got: {:?}",
        apply_decision
    );

    // =========================================================
    // STEP 10: Simulate human approval → apply to target dir
    // =========================================================

    // In a real system, the human would review the PR package and click
    // "approve". Here we simulate that by calling apply directly.
    let applied_files = connector.apply(target_dir.path()).unwrap();

    assert_eq!(applied_files.len(), 2, "should apply 2 files");
    assert!(applied_files.contains(&"config.toml".to_string()));
    assert!(applied_files.contains(&"DEPLOY.md".to_string()));

    // =========================================================
    // VERIFY: Files exist in target directory with correct content
    // =========================================================

    let applied_config = fs::read_to_string(target_dir.path().join("config.toml")).unwrap();
    assert_eq!(
        applied_config,
        "[server]\nport = 9090\nhost = \"0.0.0.0\"\n"
    );

    let applied_deploy = fs::read_to_string(target_dir.path().join("DEPLOY.md")).unwrap();
    assert!(applied_deploy.contains("Deployment Notes"));
    assert!(applied_deploy.contains("Updated port to 9090"));

    // =========================================================
    // VERIFY: Audit log has all events and hash chain is valid
    // =========================================================

    let audit_events = AuditLog::read_all(&audit_path).unwrap();

    // We expect at least 4 events:
    //   1. ToolCall for read_source (config.toml)
    //   2. ToolCall for write_patch (config.toml)
    //   3. ToolCall for write_patch (DEPLOY.md)
    //   4. Apply event
    assert!(
        audit_events.len() >= 4,
        "expected at least 4 audit events, got {}",
        audit_events.len()
    );

    // Verify the first event is a ToolCall.
    assert_eq!(audit_events[0].action, AuditAction::ToolCall);

    // Verify the last event is an Apply action.
    let last_event = audit_events.last().unwrap();
    assert_eq!(last_event.action, AuditAction::Apply);

    // Verify the hash chain is intact (no tampering).
    assert!(
        AuditLog::verify_chain(&audit_path).unwrap(),
        "audit log hash chain should be valid"
    );

    // =========================================================
    // VERIFY: Policy correctly denies unauthorized actions
    // =========================================================

    // An unknown agent should be denied.
    let rogue_decision = policy_engine.evaluate(&PolicyRequest {
        agent_id: "rogue-agent".to_string(),
        tool: "fs".to_string(),
        verb: "write_patch".to_string(),
        target_uri: "fs://workspace/hack.txt".to_string(),
    });
    assert!(matches!(rogue_decision, PolicyDecision::Deny { .. }));

    // Path traversal should be denied.
    let traversal_decision = policy_engine.evaluate(&PolicyRequest {
        agent_id: "agent-1".to_string(),
        tool: "fs".to_string(),
        verb: "read".to_string(),
        target_uri: "fs://source/../../etc/passwd".to_string(),
    });
    assert!(matches!(traversal_decision, PolicyDecision::Deny { .. }));

    // =========================================================
    // SUCCESS: The complete thesis is proven!
    // =========================================================
    //
    // We demonstrated:
    // ✓ Default-deny policy controls agent permissions
    // ✓ All writes go through staging (never direct to filesystem)
    // ✓ Each write produces a ChangeSet with diff and integrity hash
    // ✓ Changes are persisted to a durable store (JSONL)
    // ✓ A PR package bundles all changes for review
    // ✓ Apply (side effects) always requires human approval
    // ✓ After approval, changes apply correctly to target
    // ✓ Every operation is recorded in a tamper-evident audit log
    // ✓ Unauthorized agents and path traversal are blocked
}
