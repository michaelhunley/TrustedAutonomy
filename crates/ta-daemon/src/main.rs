//! # ta-daemon
//!
//! Trusted Autonomy MCP server daemon.
//!
//! Starts an MCP server on stdio that Claude Code (or any MCP client)
//! connects to. All agent tool calls flow through the gateway's policy
//! engine, staging workspace, and audit log.
//!
//! ## Usage
//!
//! Typically started automatically by the MCP client via `.mcp.json`:
//! ```json
//! {
//!   "mcpServers": {
//!     "trusted-autonomy": {
//!       "type": "stdio",
//!       "command": "cargo",
//!       "args": ["run", "-p", "ta-daemon"]
//!     }
//!   }
//! }
//! ```

use anyhow::Result;
use clap::Parser;
use rmcp::ServiceExt;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use ta_mcp_gateway::{GatewayConfig, TaGatewayServer};

/// Trusted Autonomy MCP server.
#[derive(Parser)]
#[command(name = "ta-daemon", about = "Trusted Autonomy MCP server")]
struct Cli {
    /// Project root directory (defaults to current directory).
    #[arg(long, default_value = ".")]
    project_root: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Logs go to stderr so they don't interfere with MCP on stdout.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("ta_mcp_gateway=info".parse()?)
                .add_directive("ta_daemon=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();
    let project_root = cli.project_root.canonicalize()?;

    tracing::info!("Starting Trusted Autonomy MCP server");
    tracing::info!("Project root: {}", project_root.display());

    let config = GatewayConfig::for_project(&project_root);
    let server = TaGatewayServer::new(config)?;

    tracing::info!("MCP server ready, waiting for client connection");

    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {:?}", e))?;

    service.waiting().await?;

    tracing::info!("MCP server shutting down");
    Ok(())
}
