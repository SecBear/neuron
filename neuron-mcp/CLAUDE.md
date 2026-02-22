# neuron-mcp

MCP (Model Context Protocol) integration for neuron.

## Key types
- `McpClient` — connects to MCP servers via stdio or HTTP transport
- `StdioConfig` — configuration for stdio-based MCP connections
- `HttpConfig` — configuration for HTTP-based MCP connections
- `McpToolBridge` — bridges MCP tools to `ToolDyn` for use in `ToolRegistry`
- `McpServer` — exposes a `ToolRegistry` as an MCP server
- `PaginatedList<T>` — paginated response wrapper
- `McpResource`, `McpResourceContents` — MCP resource types
- `McpPrompt`, `McpPromptArgument`, `McpPromptMessage`, `McpPromptContent`, `McpPromptResult` — MCP prompt types

## Key patterns
- Wraps `rmcp` (official Rust MCP SDK), does not reimplement the protocol
- `McpToolBridge` implements `ToolDyn` to bridge MCP tools into our type system
- `McpClient::discover_tools()` returns `Vec<Arc<dyn ToolDyn>>` for easy registry integration
- Error types convert from `rmcp::ServiceError` to `McpError`

## Conventions
- All public types re-exported from `lib.rs`
- `thiserror` errors via `McpError` from `neuron-types`
- No `unwrap()` in library code
- Native async (RPITIT), Rust 2024 edition
