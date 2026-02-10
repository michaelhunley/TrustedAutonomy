// serve.rs — Start the MCP server on stdio.
//
// This delegates to the same logic as ta-daemon, allowing users to
// start the server via `ta serve` without needing to know the binary name.

use std::path::Path;

use rmcp::ServiceExt;
use ta_mcp_gateway::{GatewayConfig, TaGatewayServer};

pub fn execute(project_root: &Path) -> anyhow::Result<()> {
    let config = GatewayConfig::for_project(project_root);
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
