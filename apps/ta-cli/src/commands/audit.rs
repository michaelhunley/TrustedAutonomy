// audit.rs — Audit subcommands: verify, tail, show, export, drift, baseline.

use clap::Subcommand;
use ta_audit::{AuditEvent, AuditLog, BaselineStore, DraftSummary, DriftSeverity};
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
    /// Show behavioral drift report for an agent (v0.4.2).
    Drift {
        /// Agent ID to check (omit with --all for all agents).
        agent_id: Option<String>,
        /// Show drift summary for all agents with stored baselines.
        #[arg(long)]
        all: bool,
        /// Path to audit log (defaults to .ta/audit.jsonl).
        #[arg(long)]
        log: Option<String>,
        /// Number of recent goals to compare against baseline.
        #[arg(long, default_value = "5")]
        window: usize,
    },
    /// Compute and store a behavioral baseline for an agent (v0.4.2).
    Baseline {
        /// Agent ID to compute baseline for.
        agent_id: String,
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

        AuditCommands::Drift {
            agent_id,
            all,
            log,
            window,
        } => {
            execute_drift(config, agent_id.as_deref(), *all, log.as_deref(), *window)?;
        }

        AuditCommands::Baseline { agent_id, log } => {
            execute_baseline(config, agent_id, log.as_deref())?;
        }
    }

    Ok(())
}

// ── Drift subcommand (v0.4.2) ──

fn baselines_dir(config: &GatewayConfig) -> std::path::PathBuf {
    config.workspace_root.join(".ta").join("baselines")
}

/// Load draft package summaries from the pr_packages directory.
fn load_draft_summaries(config: &GatewayConfig) -> anyhow::Result<Vec<DraftSummary>> {
    let dir = &config.pr_packages_dir;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut summaries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(pkg) = serde_json::from_str::<ta_changeset::DraftPackage>(&data) {
                    let rejected = matches!(pkg.status, ta_changeset::DraftStatus::Denied { .. });
                    let dep_count = pkg
                        .changes
                        .artifacts
                        .iter()
                        .filter(|a| ta_audit::drift::is_dependency_file(&a.resource_uri))
                        .count();
                    summaries.push(DraftSummary {
                        agent_id: pkg.agent_identity.agent_id,
                        artifact_count: pkg.changes.artifacts.len(),
                        risk_score: pkg.risk.risk_score,
                        rejected,
                        dependency_artifact_count: dep_count,
                    });
                }
            }
        }
    }
    Ok(summaries)
}

fn execute_drift(
    config: &GatewayConfig,
    agent_id: Option<&str>,
    all: bool,
    log_path: Option<&str>,
    window: usize,
) -> anyhow::Result<()> {
    let store = BaselineStore::new(baselines_dir(config));

    // Determine which agents to report on.
    let agents: Vec<String> = if all {
        store.list_agents()?
    } else if let Some(id) = agent_id {
        vec![id.to_string()]
    } else {
        anyhow::bail!("Provide an agent ID or use --all");
    };

    if agents.is_empty() {
        println!("No baselines found. Run `ta audit baseline <agent-id>` first.");
        return Ok(());
    }

    // Load audit events.
    let audit_path = log_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| config.audit_log.clone());
    let events = if audit_path.exists() {
        AuditLog::read_all(&audit_path)?
    } else {
        Vec::new()
    };

    // Load draft summaries.
    let drafts = load_draft_summaries(config)?;

    for agent in &agents {
        let baseline = match store.load(agent)? {
            Some(b) => b,
            None => {
                println!(
                    "{}: no baseline found (run `ta audit baseline {}`)",
                    agent, agent
                );
                continue;
            }
        };

        let report = ta_audit::drift::compute_drift(&baseline, &events, &drafts, window);

        println!("Drift report for {}", agent);
        println!("{}", "=".repeat(50));
        println!(
            "Baseline: {} goals, computed {}",
            baseline.goal_count,
            baseline.computed_at.format("%Y-%m-%d %H:%M"),
        );
        println!("Window: {} recent goals", window);
        println!(
            "Overall: {}",
            match report.overall_severity {
                DriftSeverity::Normal => "NORMAL — no significant drift",
                DriftSeverity::Warning => "WARNING — notable behavioral changes",
                DriftSeverity::Alert => "ALERT — significant behavioral divergence",
            }
        );

        if report.findings.is_empty() {
            println!("  No drift signals detected.");
        } else {
            println!();
            for finding in &report.findings {
                let icon = match finding.severity {
                    DriftSeverity::Normal => " ",
                    DriftSeverity::Warning => "!",
                    DriftSeverity::Alert => "X",
                };
                println!("[{}] {:?}: {}", icon, finding.signal, finding.description);
                if let (Some(bv), Some(cv)) = (finding.baseline_value, finding.current_value) {
                    println!("    baseline={:.2}  current={:.2}", bv, cv);
                }
            }
        }
        println!();
    }

    Ok(())
}

fn execute_baseline(
    config: &GatewayConfig,
    agent_id: &str,
    log_path: Option<&str>,
) -> anyhow::Result<()> {
    let audit_path = log_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| config.audit_log.clone());

    let events = if audit_path.exists() {
        AuditLog::read_all(&audit_path)?
    } else {
        Vec::new()
    };
    let drafts = load_draft_summaries(config)?;

    let baseline = ta_audit::drift::compute_baseline(agent_id, &events, &drafts);

    let store = BaselineStore::new(baselines_dir(config));
    store.save(&baseline)?;

    println!("Baseline stored for {}", agent_id);
    println!("  Goals in sample: {}", baseline.goal_count);
    println!("  Resource patterns: {}", baseline.resource_patterns.len());
    println!("  Avg artifact count: {:.1}", baseline.avg_artifact_count);
    println!("  Avg risk score: {:.1}", baseline.avg_risk_score);
    println!(
        "  Escalation rate: {:.1}%",
        baseline.escalation_rate * 100.0
    );
    println!("  Rejection rate: {:.1}%", baseline.rejection_rate * 100.0);
    println!(
        "  Stored at: {}",
        baselines_dir(config)
            .join(format!("{}.json", agent_id))
            .display()
    );

    Ok(())
}

// ── Existing subcommands ──

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
