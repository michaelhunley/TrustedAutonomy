// serve.rs — Start the MCP server on stdio.
//
// This delegates to the same logic as ta-daemon, allowing users to
// start the server via `ta serve` without needing to know the binary name.

use std::path::Path;

use rmcp::ServiceExt;
use ta_mcp_gateway::{GatewayConfig, TaGatewayServer};

pub fn execute(project_root: &Path) -> anyhow::Result<()> {
    // Honor TA_PROJECT_ROOT env var if set (used when launched as MCP server
    // subprocess via .mcp.json). Falls back to --project-root CLI arg.
    let effective_root = std::env::var("TA_PROJECT_ROOT")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| project_root.to_path_buf());
    let config = GatewayConfig::for_project(&effective_root);
    let server = TaGatewayServer::new(config)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let transport = rmcp::transport::stdio();
        let server_handle = server
            .serve(transport)
            .await
            .map_err(|e| anyhow::anyhow!("MCP server error: {}", e))?;
        let _ = server_handle.waiting().await;
        Ok::<(), anyhow::Error>(())
    })
}
