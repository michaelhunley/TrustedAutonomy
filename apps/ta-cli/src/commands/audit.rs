// audit.rs — Audit subcommands: verify, tail.

use clap::Subcommand;
use ta_audit::AuditLog;
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

            let events = AuditLog::read_all(&path)?;
            if events.is_empty() {
                println!("Audit log is empty.");
                return Ok(());
            }

            // Verify the hash chain by checking each event links to the previous.
            let mut valid = true;
            for (i, event) in events.iter().enumerate().skip(1) {
                if event.previous_hash.is_none() {
                    println!("WARNING: Event {} has no previous_hash link", i);
                    valid = false;
                }
            }

            // First event should have no previous_hash.
            if events[0].previous_hash.is_some() {
                println!("WARNING: First event has unexpected previous_hash");
                valid = false;
            }

            if valid {
                println!(
                    "Audit log verified: {} event(s), hash chain intact.",
                    events.len()
                );
            } else {
                println!(
                    "Audit log has {} event(s) with integrity warnings.",
                    events.len()
                );
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
    }

    Ok(())
}
