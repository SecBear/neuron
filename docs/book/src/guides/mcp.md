# MCP Integration

`neuron-mcp` connects your agent to external tool servers using the
[Model Context Protocol](https://modelcontextprotocol.io) (MCP). It wraps the
`rmcp` crate (the official Rust MCP SDK) and bridges MCP tools into neuron's
`ToolRegistry` so they appear like any other tool to the agent loop.

## Quick Example

```rust,ignore
use std::sync::Arc;
use neuron_mcp::{McpClient, McpToolBridge, StdioConfig};
use neuron_tool::ToolRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to an MCP server via stdio
    let client = Arc::new(McpClient::connect_stdio(StdioConfig {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string(), "/tmp".to_string()],
        env: vec![],
    }).await?);

    // Discover tools and register them
    let tools = McpToolBridge::discover(&client).await?;
    let mut registry = ToolRegistry::new();
    for tool in tools {
        registry.register_dyn(tool);
    }

    Ok(())
}
```

## API Walkthrough

### McpClient

`McpClient` manages the connection to an MCP server. Two transports are
supported:

**Stdio** -- spawns a child process and communicates over stdin/stdout:

```rust,ignore
use neuron_mcp::{McpClient, StdioConfig};

let client = McpClient::connect_stdio(StdioConfig {
    command: "npx".to_string(),
    args: vec!["-y".to_string(), "@modelcontextprotocol/server-everything".to_string()],
    env: vec![("NODE_ENV".to_string(), "production".to_string())],
}).await?;
```

**Streamable HTTP** -- connects to a remote MCP server over HTTP with SSE:

```rust,ignore
use neuron_mcp::{McpClient, HttpConfig};

let client = McpClient::connect_http(HttpConfig {
    url: "http://localhost:8080/mcp".to_string(),
    auth_header: Some("Bearer my-token".to_string()),
    headers: vec![],
}).await?;
```

Once connected, `McpClient` provides methods for all MCP operations:

| Method | Description |
|--------|-------------|
| `list_tools(cursor)` | List available tools (paginated) |
| `list_all_tools()` | List all tools (fetches every page) |
| `call_tool(name, arguments)` | Call a tool with a JSON argument map |
| `call_tool_json(name, value)` | Convenience: accepts `serde_json::Value` |
| `list_resources(cursor)` | List available resources |
| `read_resource(uri)` | Read a resource by URI |
| `list_prompts(cursor)` | List available prompt templates |
| `get_prompt(name, arguments)` | Retrieve an expanded prompt |
| `is_closed()` | Check if the transport is closed |
| `peer()` | Access the underlying `rmcp` peer for advanced use |

### McpToolBridge

`McpToolBridge` bridges a single MCP tool into neuron's `ToolDyn` trait. When
the agent loop calls a bridged tool, the call is forwarded to the MCP server
via `McpClient::call_tool`.

The typical workflow uses `McpToolBridge::discover()`, which lists all tools
from the server and returns them as `Arc<dyn ToolDyn>` ready for registration:

```rust,ignore
use std::sync::Arc;
use neuron_mcp::{McpClient, McpToolBridge};
use neuron_tool::ToolRegistry;

let client = Arc::new(McpClient::connect_stdio(config).await?);

// Discover returns Vec<Arc<dyn ToolDyn>>
let bridges = McpToolBridge::discover(&client).await?;

let mut registry = ToolRegistry::new();
for bridge in bridges {
    registry.register_dyn(bridge);
}
```

An equivalent convenience method exists on `McpClient` itself:

```rust,ignore
let tools = McpClient::discover_tools(&client).await?;
```

You can also bridge a single known tool manually:

```rust,ignore
use neuron_mcp::McpToolBridge;

let bridge = McpToolBridge::new(Arc::clone(&client), tool_definition);
registry.register_dyn(Arc::new(bridge));
```

### McpServer

`McpServer` does the reverse: it exposes a neuron `ToolRegistry` as an MCP
server, making your tools available to any MCP client.

```rust,ignore
use neuron_mcp::McpServer;
use neuron_tool::ToolRegistry;

let mut registry = ToolRegistry::new();
// ... register your tools ...

let server = McpServer::new(registry)
    .with_name("my-agent-tools")
    .with_version("1.0.0")
    .with_instructions("Tools for file manipulation");

// Serve over stdio (blocks until client disconnects)
server.serve_stdio().await?;
```

The server handles `tools/list` and `tools/call` MCP requests by delegating
to the underlying `ToolRegistry`.

### Configuration Types

- **`StdioConfig`** -- `command`, `args`, `env` for spawning a child process
- **`HttpConfig`** -- `url`, `auth_header`, `headers` for HTTP connections
- **`PaginatedList<T>`** -- generic wrapper with `items` and `next_cursor`

### MCP-Specific Types

These types represent MCP protocol objects:

- **`McpResource`** -- `uri`, `name`, `title`, `description`, `mime_type`
- **`McpResourceContents`** -- `uri`, `mime_type`, `text` or `blob`
- **`McpPrompt`** -- `name`, `title`, `description`, `arguments`
- **`McpPromptArgument`** -- `name`, `description`, `required`

### Error Handling

All MCP operations return `Result<_, McpError>`. The variants are:

- `McpError::Connection` -- failed to connect (process spawn or HTTP)
- `McpError::Initialization` -- MCP handshake failed
- `McpError::ToolCall` -- a tool call returned an error
- `McpError::Transport` -- transport-level communication error

## Advanced Usage

### Mixing MCP and Native Tools

MCP tools and native tools live side by side in the same `ToolRegistry`. The
agent loop cannot tell the difference:

```rust,ignore
use neuron_mcp::{McpClient, McpToolBridge, StdioConfig};
use neuron_tool::ToolRegistry;

let mut registry = ToolRegistry::new();

// Register a native tool
registry.register(MyNativeTool);

// Register MCP tools from a filesystem server
let fs_client = Arc::new(McpClient::connect_stdio(fs_config).await?);
for tool in McpToolBridge::discover(&fs_client).await? {
    registry.register_dyn(tool);
}

// Register MCP tools from a different server
let db_client = Arc::new(McpClient::connect_http(db_config).await?);
for tool in McpToolBridge::discover(&db_client).await? {
    registry.register_dyn(tool);
}

// All tools are now available to the agent loop
```

### Accessing the Raw rmcp Peer

For operations not covered by `McpClient`'s methods, access the underlying
`rmcp::Peer` directly:

```rust,ignore
let peer = client.peer();
// Use any rmcp method directly
```

### Tool Annotations

MCP tools can carry behavioral annotations (read-only, destructive, idempotent,
open-world). These are preserved during bridging and available on the
`ToolDefinition`:

```rust,ignore
let tools = client.list_all_tools().await?;
for tool in &tools {
    if let Some(ann) = &tool.annotations {
        println!("{}: read_only={:?}", tool.name, ann.read_only_hint);
    }
}
```

## API Docs

Full API documentation: [neuron-mcp on docs.rs](https://docs.rs/neuron-mcp)
