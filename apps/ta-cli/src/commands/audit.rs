// audit.rs — Audit subcommands: verify, tail, show, export.

use clap::Subcommand;
use ta_audit::{AuditEvent, AuditLog};
use ta_mcp_gateway::GatewayConfig;

#[derive(Subcommand)]
pub enum AuditCommands {
    /// Verify the audit log hash chain integrity.
    Verify {
        /// Path to audit log (defaults to .ta/audit.jsonl).
        #[arg(long)]
        log: Option<String>,
    },
    /// Show recent audit events.
    Tail {
        /// Path to audit log (defaults to .ta/audit.jsonl).
        #[arg(long)]
        log: Option<String>,
        /// Number of events to show.
        #[arg(short, default_value = "10")]
        n: usize,
    },
    /// Display the decision trail for a goal with reasoning (v0.3.3).
    Show {
        /// Goal ID to display decision trail for.
        goal_id: String,
        /// Path to audit log (defaults to .ta/audit.jsonl).
        #[arg(long)]
        log: Option<String>,
    },
    /// Export structured audit data for compliance reporting (v0.3.3).
    Export {
        /// Goal ID to export.
        goal_id: String,
        /// Output format.
        #[arg(long, default_value = "json")]
        format: ExportFormat,
        /// Path to audit log (defaults to .ta/audit.jsonl).
        #[arg(long)]
        log: Option<String>,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ExportFormat {
    Json,
}

pub fn execute(cmd: &AuditCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    match cmd {
        AuditCommands::Verify { log } => {
            let path = log
                .as_ref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| config.audit_log.clone());

            if !path.exists() {
                println!("No audit log found at {}", path.display());
                return Ok(());
            }

            // Verify using the real hash-chain verification (recomputes hashes).
            match AuditLog::verify_chain(&path) {
                Ok(_) => {
                    let events = AuditLog::read_all(&path)?;
                    println!(
                        "Audit log verified: {} event(s), hash chain intact.",
                        events.len()
                    );
                }
                Err(ta_audit::AuditError::IntegrityViolation {
                    line,
                    expected,
                    actual,
                }) => {
                    println!("INTEGRITY VIOLATION at line {}:", line);
                    println!("  Expected previous_hash: {}", expected);
                    println!("  Actual previous_hash:   {}", actual);
                    println!();
                    println!("The audit log may have been tampered with.");
                    anyhow::bail!("Audit log integrity check failed");
                }
                Err(e) => return Err(e.into()),
            }
        }

        AuditCommands::Tail { log, n } => {
            let path = log
                .as_ref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| config.audit_log.clone());

            if !path.exists() {
                println!("No audit log found at {}", path.display());
                return Ok(());
            }

            let events = AuditLog::read_all(&path)?;
            let start = events.len().saturating_sub(*n);
            let recent = &events[start..];

            if recent.is_empty() {
                println!("No audit events.");
                return Ok(());
            }

            println!(
                "{:<26} {:<12} {:<14} TARGET",
                "TIMESTAMP", "AGENT", "ACTION"
            );
            println!("{}", "-".repeat(80));

            for event in recent {
                println!(
                    "{:<26} {:<12} {:<14} {}",
                    event.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    event.agent_id,
                    format!("{:?}", event.action),
                    event.target_uri.as_deref().unwrap_or("-"),
                );
            }
        }

        AuditCommands::Show { goal_id, log } => {
            show_decision_trail(config, goal_id, log.as_deref())?;
        }

        AuditCommands::Export {
            goal_id,
            format,
            log,
        } => {
            export_audit(config, goal_id, format, log.as_deref())?;
        }
    }

    Ok(())
}

/// Filter events related to a specific goal.
fn events_for_goal(events: &[AuditEvent], goal_id: &str) -> Vec<AuditEvent> {
    events
        .iter()
        .filter(|e| {
            // Check agent_id or metadata for goal reference.
            e.agent_id.contains(goal_id)
                || e.target_uri
                    .as_deref()
                    .map(|u| u.contains(goal_id))
                    .unwrap_or(false)
                || {
                    // Check metadata for goal_id reference.
                    let meta = e.metadata.to_string();
                    meta.contains(goal_id)
                }
        })
        .cloned()
        .collect()
}

/// Display the decision trail for a goal with reasoning (v0.3.3).
fn show_decision_trail(
    config: &GatewayConfig,
    goal_id: &str,
    log_path: Option<&str>,
) -> anyhow::Result<()> {
    let path = log_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| config.audit_log.clone());

    if !path.exists() {
        println!("No audit log found at {}", path.display());
        return Ok(());
    }

    let all_events = AuditLog::read_all(&path)?;
    let events = events_for_goal(&all_events, goal_id);

    if events.is_empty() {
        // Show all events if no goal-specific filter matches — the goal_id
        // might be a prefix or might match the draft package instead.
        println!("No events found for goal {}.", goal_id);
        println!();
        println!(
            "Showing all {} events with decision reasoning:",
            all_events.len()
        );

        let reasoned: Vec<_> = all_events
            .iter()
            .filter(|e| e.reasoning.is_some())
            .collect();
        if reasoned.is_empty() {
            println!("  No events have decision reasoning attached.");
            println!("  Reasoning is captured during policy evaluation (v0.3.3).");
        } else {
            print_events_with_reasoning(&reasoned);
        }
        return Ok(());
    }

    println!("Decision trail for goal {}", goal_id);
    println!("{}", "=".repeat(60));
    println!("{} event(s) found", events.len());
    println!();

    let event_refs: Vec<&AuditEvent> = events.iter().collect();
    print_events_with_reasoning(&event_refs);

    // Count events with reasoning.
    let reasoning_count = events.iter().filter(|e| e.reasoning.is_some()).count();
    println!();
    println!(
        "Summary: {} total events, {} with decision reasoning",
        events.len(),
        reasoning_count
    );

    Ok(())
}

/// Print events with their reasoning details.
fn print_events_with_reasoning(events: &[&AuditEvent]) {
    for event in events {
        println!(
            "[{}] {:?} by {}",
            event.timestamp.format("%Y-%m-%d %H:%M:%S"),
            event.action,
            event.agent_id,
        );
        if let Some(target) = &event.target_uri {
            println!("  Target: {}", target);
        }
        if let Some(reasoning) = &event.reasoning {
            println!("  Rationale: {}", reasoning.rationale);
            if !reasoning.applied_principles.is_empty() {
                println!("  Principles: {}", reasoning.applied_principles.join(", "));
            }
            for (i, alt) in reasoning.alternatives.iter().enumerate() {
                println!(
                    "  Alternative {}: {} (rejected: {}{})",
                    i + 1,
                    alt.description,
                    alt.rejected_reason,
                    alt.score
                        .map(|s| format!(", score: {:.2}", s))
                        .unwrap_or_default(),
                );
            }
        }
        println!();
    }
}

/// Export audit data for compliance reporting (v0.3.3).
fn export_audit(
    config: &GatewayConfig,
    goal_id: &str,
    _format: &ExportFormat,
    log_path: Option<&str>,
) -> anyhow::Result<()> {
    let path = log_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| config.audit_log.clone());

    if !path.exists() {
        anyhow::bail!("No audit log found at {}", path.display());
    }

    let all_events = AuditLog::read_all(&path)?;
    let events = events_for_goal(&all_events, goal_id);

    // Build structured export.
    let export = serde_json::json!({
        "export_version": "1.0.0",
        "goal_id": goal_id,
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "total_events": events.len(),
        "events_with_reasoning": events.iter().filter(|e| e.reasoning.is_some()).count(),
        "standards_alignment": {
            "iso_42001": "Decision processes documented with rationale (A.6.2.3)",
            "ieee_7001": "Autonomous system decisions explainable to stakeholders",
            "nist_ai_rmf": "MAP 1.1 (purpose documentation), GOVERN 1.3 (decision documentation)"
        },
        "events": events,
    });

    println!("{}", serde_json::to_string_pretty(&export)?);

    Ok(())
}
