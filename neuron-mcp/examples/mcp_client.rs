//! Connect to an MCP server via stdio, discover tools, and register them.
//!
//! This example requires a running MCP server. For instance, install the
//! filesystem server and run:
//!
//! ```sh
//! cargo run --example mcp_client -p neuron-mcp
//! ```
//!
//! The example spawns `npx @modelcontextprotocol/server-filesystem /tmp`
//! as a child process, so you need Node.js and npx available on your PATH.

use std::sync::Arc;

use neuron_mcp::{McpClient, McpToolBridge, StdioConfig};
use neuron_tool::ToolRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Configure the MCP server to spawn via stdio.
    let config = StdioConfig {
        command: "npx".to_string(),
        args: vec![
            "-y".to_string(),
            "@modelcontextprotocol/server-filesystem".to_string(),
            "/tmp".to_string(),
        ],
        env: vec![],
    };

    // 2. Connect to the MCP server via stdio transport.
    println!("Connecting to MCP filesystem server...");
    let client = Arc::new(McpClient::connect_stdio(config).await?);
    println!("Connected.");

    // 3. Discover all tools exposed by the server.
    let tools = McpToolBridge::discover(&client).await?;
    println!("Discovered {} tool(s):", tools.len());

    // 4. Register discovered tools into a ToolRegistry.
    let mut registry = ToolRegistry::new();
    for tool in &tools {
        println!("  - {} : {}", tool.name(), tool.definition().description);
        registry.register_dyn(Arc::clone(tool));
    }

    // 5. List all available tools in the registry.
    println!("\nRegistered tool definitions:");
    for def in registry.definitions() {
        println!("  [{}] {}", def.name, def.description);
    }

    Ok(())
}
