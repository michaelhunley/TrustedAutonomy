//! `ta sync` — Pull upstream changes into the local workspace.
//!
//! Calls the configured `SourceAdapter::sync_upstream()` to make the local
//! workspace current with its upstream. Emits `sync_completed` or
//! `sync_conflict` events through the TA event system.

use ta_mcp_gateway::GatewayConfig;
use ta_submit::{select_adapter_with_sync, SourceAdapter, WorkflowConfig};

/// Execute `ta sync`.
///
/// Resolves the VCS adapter from `.ta/workflow.toml`, calls `sync_upstream()`,
/// and reports the result. Warns if staging workspaces are active.
pub fn execute(config: &GatewayConfig) -> anyhow::Result<()> {
    let project_root = &config.workspace_root;

    // Load workflow config.
    let workflow_config_path = project_root.join(".ta/workflow.toml");
    let workflow_config = WorkflowConfig::load_or_default(&workflow_config_path);

    // Select adapter with sync config.
    let adapter: Box<dyn SourceAdapter> = select_adapter_with_sync(
        project_root,
        &workflow_config.submit,
        &workflow_config.source.sync,
    );

    if adapter.name() == "none" {
        eprintln!(
            "No VCS adapter detected. Configure [submit].adapter in .ta/workflow.toml \
             or run from a Git/SVN/Perforce working directory."
        );
        return Err(anyhow::anyhow!(
            "No VCS adapter available for sync. \
             Configure [submit].adapter in .ta/workflow.toml."
        ));
    }

    println!("Syncing upstream using {} adapter...", adapter.name());
    println!(
        "  Strategy: {}, Remote: {}, Branch: {}",
        workflow_config.source.sync.strategy,
        workflow_config.source.sync.remote,
        workflow_config.source.sync.branch,
    );

    // Warn about active staging workspaces.
    let staging_dir = project_root.join(".ta/staging");
    if staging_dir.exists() {
        let active_count = std::fs::read_dir(&staging_dir)
            .map(|entries| entries.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        if active_count > 0 {
            eprintln!(
                "\nWarning: {} active staging workspace(s) detected.\n  \
                 Syncing the source project may cause staging workspaces to \
                 diverge from the source. Consider completing or discarding \
                 active goals before syncing, or use `ta run --follow-up` \
                 to rebase after sync.",
                active_count
            );
        }
    }

    // Run the sync.
    match adapter.sync_upstream() {
        Ok(result) => {
            if result.is_clean() {
                if result.updated {
                    println!("\n[ok] {}", result.message);
                    println!("  {} new commit(s) incorporated.", result.new_commits);

                    // Emit sync_completed event.
                    emit_sync_event(config, &*adapter, &result);
                } else {
                    println!("\n[ok] Already up to date.");
                }
            } else {
                // Conflicts detected.
                eprintln!("\n[conflict] {}", result.message);
                eprintln!("\nConflicting files:");
                for f in &result.conflicts {
                    eprintln!("  C  {}", f);
                }
                eprintln!("\nResolve conflicts, then `git add <files>` and `git commit`.");

                // Emit sync_conflict event.
                emit_conflict_event(config, &*adapter, &result);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("\nSync failed: {}", e);
            eprintln!(
                "\nTroubleshooting:\n  \
                 - Check network connectivity\n  \
                 - Verify remote '{}' exists: `git remote -v`\n  \
                 - Verify branch '{}' exists on remote\n  \
                 - Check [source.sync] in .ta/workflow.toml",
                workflow_config.source.sync.remote, workflow_config.source.sync.branch,
            );
            Err(anyhow::anyhow!("Sync failed: {}", e))
        }
    }
}

/// Emit a `sync_completed` event through the TA event store.
fn emit_sync_event(
    config: &GatewayConfig,
    adapter: &dyn SourceAdapter,
    result: &ta_submit::SyncResult,
) {
    use ta_events::schema::{EventEnvelope, SessionEvent};
    use ta_events::store::{EventStore, FsEventStore};

    let event = SessionEvent::SyncCompleted {
        adapter: adapter.name().to_string(),
        new_commits: result.new_commits,
        message: result.message.clone(),
    };

    let events_dir = config.workspace_root.join(".ta").join("events");
    let store = FsEventStore::new(&events_dir);
    if let Err(e) = store.append(&EventEnvelope::new(event)) {
        tracing::warn!(error = %e, "Failed to store sync_completed event");
    }
}

/// Emit a `sync_conflict` event through the TA event store.
fn emit_conflict_event(
    config: &GatewayConfig,
    adapter: &dyn SourceAdapter,
    result: &ta_submit::SyncResult,
) {
    use ta_events::schema::{EventEnvelope, SessionEvent};
    use ta_events::store::{EventStore, FsEventStore};

    let event = SessionEvent::SyncConflict {
        adapter: adapter.name().to_string(),
        conflicts: result.conflicts.clone(),
        message: result.message.clone(),
    };

    let events_dir = config.workspace_root.join(".ta").join("events");
    let store = FsEventStore::new(&events_dir);
    if let Err(e) = store.append(&EventEnvelope::new(event)) {
        tracing::warn!(error = %e, "Failed to store sync_conflict event");
    }
}

#[cfg(test)]
mod tests {
    use ta_submit::{NoneAdapter, SourceAdapter};

    #[test]
    fn none_adapter_sync_returns_not_updated() {
        let adapter = NoneAdapter::new();
        let result = adapter.sync_upstream().unwrap();
        assert!(!result.updated);
        assert!(result.is_clean());
    }
}
