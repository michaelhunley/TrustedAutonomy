// terms.rs — First-run terms acceptance gate.
//
// On first run (or when DISCLAIMER.md changes), the user must accept the terms.
// Acceptance state is stored at ~/.config/ta/terms.json with a SHA-256 hash
// of the disclaimer text, so updated terms trigger re-acceptance.

use std::io::Write;
use std::path::PathBuf;

/// The disclaimer text, embedded at compile time.
const DISCLAIMER: &str = include_str!("../../../../DISCLAIMER.md");

/// Where acceptance state is stored.
fn terms_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("ta")
            .join("terms.json"),
    )
}

/// Compute SHA-256 hash of the disclaimer (first 16 hex chars for brevity).
fn disclaimer_hash() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(DISCLAIMER.as_bytes());
    let result = hasher.finalize();
    hex_encode(&result[..8])
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TermsAcceptance {
    accepted: bool,
    disclaimer_hash: String,
    accepted_at: String,
    version: String,
}

/// Check if terms have been accepted for the current disclaimer version.
/// Returns Ok(()) if accepted, Err with message if not.
pub fn check_accepted() -> anyhow::Result<()> {
    let path = match terms_path() {
        Some(p) => p,
        None => return Ok(()), // Can't determine home dir — skip check.
    };

    if !path.exists() {
        return Err(anyhow::anyhow!(
            "Terms not yet accepted. Run `ta accept-terms` or use `ta --accept-terms <command>`."
        ));
    }

    let content = std::fs::read_to_string(&path)?;
    let acceptance: TermsAcceptance = serde_json::from_str(&content)?;

    if !acceptance.accepted || acceptance.disclaimer_hash != disclaimer_hash() {
        return Err(anyhow::anyhow!(
            "Terms have been updated since your last acceptance. Run `ta accept-terms` to review and accept."
        ));
    }

    Ok(())
}

/// Display the disclaimer and prompt the user for interactive acceptance.
pub fn prompt_and_accept() -> anyhow::Result<()> {
    // Print the disclaimer.
    println!("{}", DISCLAIMER);
    println!("─────────────────────────────────────────────────────");
    print!("Do you accept these terms? [y/N] ");
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    let answer = input.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        return Err(anyhow::anyhow!(
            "Terms not accepted. Cannot continue without accepting the terms of use."
        ));
    }

    record_acceptance()?;
    println!("\nTerms accepted. You can now use ta.");
    Ok(())
}

/// Accept terms non-interactively (for CI / scripted usage).
pub fn accept_non_interactive() -> anyhow::Result<()> {
    record_acceptance()?;
    println!("Terms accepted (non-interactive).");
    Ok(())
}

/// Write the acceptance record to disk.
fn record_acceptance() -> anyhow::Result<()> {
    let path = terms_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory for terms storage"))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let acceptance = TermsAcceptance {
        accepted: true,
        disclaimer_hash: disclaimer_hash(),
        accepted_at: chrono::Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let json = serde_json::to_string_pretty(&acceptance)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// Show the current acceptance status.
pub fn show_status() -> anyhow::Result<()> {
    let path = match terms_path() {
        Some(p) => p,
        None => {
            println!("Cannot determine terms file location.");
            return Ok(());
        }
    };

    if !path.exists() {
        println!("Terms have not been accepted yet.");
        println!("Run `ta accept-terms` to review and accept.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let acceptance: TermsAcceptance = serde_json::from_str(&content)?;
    let current_hash = disclaimer_hash();

    if acceptance.disclaimer_hash == current_hash {
        println!("Terms accepted: {}", acceptance.accepted_at);
        println!("Version: {}", acceptance.version);
        println!("Status: current");
    } else {
        println!("Terms were accepted for a previous version.");
        println!("Last accepted: {}", acceptance.accepted_at);
        println!("Run `ta accept-terms` to accept the updated terms.");
    }

    Ok(())
}

/// View the full disclaimer text.
pub fn view_terms() {
    println!("{}", DISCLAIMER);
}

// ── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disclaimer_hash_is_stable() {
        let h1 = disclaimer_hash();
        let h2 = disclaimer_hash();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16); // 8 bytes = 16 hex chars
    }

    #[test]
    fn disclaimer_is_not_empty() {
        assert!(!DISCLAIMER.is_empty());
        assert!(DISCLAIMER.contains("Trusted Autonomy"));
        assert!(DISCLAIMER.contains("WITHOUT WARRANTY"));
    }

    #[test]
    fn acceptance_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("terms.json");

        let acceptance = TermsAcceptance {
            accepted: true,
            disclaimer_hash: disclaimer_hash(),
            accepted_at: "2026-02-11T00:00:00Z".to_string(),
            version: "0.2.2-alpha".to_string(),
        };

        let json = serde_json::to_string_pretty(&acceptance).unwrap();
        std::fs::write(&path, &json).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: TermsAcceptance = serde_json::from_str(&content).unwrap();
        assert!(loaded.accepted);
        assert_eq!(loaded.disclaimer_hash, disclaimer_hash());
    }
}
