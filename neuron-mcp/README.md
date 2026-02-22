# neuron-mcp

Model Context Protocol (MCP) integration for the neuron agent blocks ecosystem.
Wraps the official Rust MCP SDK (`rmcp`) and bridges MCP tools, resources, and
prompts into neuron's type system. Provides both client and server
functionality.

## Key Types

- `McpClient` -- connects to MCP servers via stdio (`connect_stdio`) or
  Streamable HTTP (`connect_http`). Provides methods for listing and calling
  tools, reading resources, and fetching prompts.
- `McpToolBridge` -- bridges a single MCP tool to the `ToolDyn` trait so it can
  be registered in a `ToolRegistry` and used alongside native tools.
- `McpServer` -- exposes a `ToolRegistry` as an MCP server, handling
  `tools/list` and `tools/call` requests.
- `StdioConfig` / `HttpConfig` -- connection configuration structs.
- `PaginatedList<T>` -- paginated response wrapper with `items` and
  `next_cursor`.
- `McpResource`, `McpResourceContents`, `McpPrompt` -- MCP resource and prompt
  types.

## Features

- Full MCP client: tools, resources, prompts, pagination.
- Full MCP server: expose any `ToolRegistry` over stdio.
- Automatic tool discovery via `McpToolBridge::discover` or
  `McpClient::discover_tools`.
- Supports both stdio (child process) and Streamable HTTP transports.
- Error types convert cleanly from `rmcp::ServiceError` to `McpError`.

## Usage

```rust,no_run
use std::sync::Arc;
use neuron_mcp::{McpClient, McpToolBridge, StdioConfig};
use neuron_tool::ToolRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to an MCP server via stdio
    let client = Arc::new(McpClient::connect_stdio(StdioConfig {
        command: "npx".to_string(),
        args: vec![
            "-y".to_string(),
            "@modelcontextprotocol/server-everything".to_string(),
        ],
        env: vec![],
    }).await?);

    // Discover all tools and get them as ToolDyn trait objects
    let tools = McpToolBridge::discover(&client).await?;

    // Register discovered tools into a ToolRegistry
    let mut registry = ToolRegistry::new();
    for tool in tools {
        registry.register_dyn(tool);
    }

    // List registered tool names
    for def in registry.definitions() {
        println!("tool: {}", def.name);
    }
    Ok(())
}
```

## Part of neuron

This crate is one block in the [neuron](https://github.com/axiston/neuron)
composable agent toolkit. It depends on `neuron-types`, `neuron-tool`, and
`rmcp`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
