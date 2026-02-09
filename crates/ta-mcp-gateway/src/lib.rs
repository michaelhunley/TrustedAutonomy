//! # ta-mcp-gateway
//!
//! In-process MCP (Model Context Protocol) gateway for Trusted Autonomy.
//!
//! Currently a trait-based in-process module. Will be extended to a full
//! JSON-RPC 2.0 MCP server in a future phase. The gateway enforces
//! capability scope, stages mutations, and routes tool calls to connectors.

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // Stub test — will be replaced with real tests during implementation.
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
