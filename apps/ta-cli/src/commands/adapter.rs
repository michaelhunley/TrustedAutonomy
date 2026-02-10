// adapter.rs — Adapter subcommands: list, install.
//
// Adapters are config templates that generate agent-specific integration
// files. `ta adapter install claude-code` generates `.mcp.json` and
// `.ta/config.toml` so Claude Code can connect to the TA MCP server.

use std::fs;
use std::path::Path;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum AdapterCommands {
    /// List available agent adapters.
    List,
    /// Install an adapter for a specific agent system.
    Install {
        /// Adapter name (e.g., "claude-code").
        name: String,
    },
}

pub fn execute(cmd: &AdapterCommands, project_root: &Path) -> anyhow::Result<()> {
    match cmd {
        AdapterCommands::List => list_adapters(),
        AdapterCommands::Install { name } => install_adapter(name, project_root),
    }
}

fn list_adapters() -> anyhow::Result<()> {
    println!("Available adapters:");
    println!();
    println!("  claude-code    Claude Code (MCP stdio server)");
    println!("  generic-mcp    Generic MCP client");
    println!();
    println!("Install with: ta adapter install <name>");
    Ok(())
}

fn install_adapter(name: &str, project_root: &Path) -> anyhow::Result<()> {
    match name {
        "claude-code" => install_claude_code(project_root),
        "generic-mcp" => install_generic_mcp(project_root),
        _ => {
            anyhow::bail!(
                "Unknown adapter: {}. Run `ta adapter list` to see available adapters.",
                name
            );
        }
    }
}

fn install_claude_code(project_root: &Path) -> anyhow::Result<()> {
    // Generate .mcp.json for Claude Code auto-discovery.
    let mcp_json_path = project_root.join(".mcp.json");
    if mcp_json_path.exists() {
        println!("  .mcp.json already exists — skipping (check manually)");
    } else {
        let mcp_json = serde_json::json!({
            "mcpServers": {
                "trusted-autonomy": {
                    "type": "stdio",
                    "command": "cargo",
                    "args": ["run", "-p", "ta-daemon", "--"],
                    "env": {
                        "TA_LOG_LEVEL": "info"
                    }
                }
            }
        });
        fs::write(&mcp_json_path, serde_json::to_string_pretty(&mcp_json)?)?;
        println!("  Created .mcp.json");
    }

    // Generate .ta/config.toml with default settings.
    install_ta_config(project_root)?;

    println!();
    println!("Claude Code adapter installed!");
    println!();
    println!("Next steps:");
    println!("  1. Build TA:  cargo build --workspace");
    println!("  2. Start Claude Code in this directory");
    println!("  3. TA tools will appear alongside built-in tools");
    println!("  4. Agent stages changes through ta_fs_write");
    println!("  5. Review with: ta pr list / ta pr view <id>");
    println!("  6. Approve:    ta pr approve <id>");
    println!("  7. Apply:      ta pr apply <id>");

    Ok(())
}

fn install_generic_mcp(project_root: &Path) -> anyhow::Result<()> {
    install_ta_config(project_root)?;

    println!();
    println!("Generic MCP adapter installed!");
    println!();
    println!("Start the MCP server with:");
    println!("  cargo run -p ta-daemon -- --project-root .");
    println!();
    println!("Connect your MCP client to the server via stdio.");

    Ok(())
}

fn install_ta_config(project_root: &Path) -> anyhow::Result<()> {
    let ta_dir = project_root.join(".ta");
    fs::create_dir_all(&ta_dir)?;

    let config_path = ta_dir.join("config.toml");
    if config_path.exists() {
        println!("  .ta/config.toml already exists — skipping");
    } else {
        let config_toml = r#"# Trusted Autonomy configuration
#
# This file is created by `ta adapter install`. Edit as needed.

[workspace]
root = "."
staging_dir = ".ta/staging"
store_dir = ".ta/store"
audit_log = ".ta/audit.jsonl"

[policy]
default_template = "developer"
manifest_ttl_hours = 8

[notifications]
# Uncomment to enable notifications:
# discord_webhook = "https://discord.com/api/webhooks/..."
# email_to = "you@example.com"
"#;
        fs::write(&config_path, config_toml)?;
        println!("  Created .ta/config.toml");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn list_adapters_succeeds() {
        list_adapters().unwrap();
    }

    #[test]
    fn install_claude_code_creates_files() {
        let dir = TempDir::new().unwrap();
        install_claude_code(dir.path()).unwrap();

        assert!(dir.path().join(".mcp.json").exists());
        assert!(dir.path().join(".ta/config.toml").exists());

        // Verify .mcp.json content.
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join(".mcp.json")).unwrap())
                .unwrap();
        assert!(json["mcpServers"]["trusted-autonomy"].is_object());
        assert_eq!(json["mcpServers"]["trusted-autonomy"]["type"], "stdio");
    }

    #[test]
    fn install_claude_code_skips_existing_mcp_json() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".mcp.json"), "existing").unwrap();

        install_claude_code(dir.path()).unwrap();

        // Should not overwrite.
        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        assert_eq!(content, "existing");
    }

    #[test]
    fn install_generic_mcp_creates_config() {
        let dir = TempDir::new().unwrap();
        install_generic_mcp(dir.path()).unwrap();

        assert!(dir.path().join(".ta/config.toml").exists());
        // Should NOT create .mcp.json.
        assert!(!dir.path().join(".mcp.json").exists());
    }

    #[test]
    fn unknown_adapter_fails() {
        let dir = TempDir::new().unwrap();
        let result = install_adapter("nonexistent", dir.path());
        assert!(result.is_err());
    }
}
