# neuron-mcp

> Model Context Protocol (MCP) bridge for neuron

[![crates.io](https://img.shields.io/crates/v/neuron-mcp.svg)](https://crates.io/crates/neuron-mcp)
[![docs.rs](https://docs.rs/neuron-mcp/badge.svg)](https://docs.rs/neuron-mcp)
[![license](https://img.shields.io/crates/l/neuron-mcp.svg)](LICENSE-MIT)

## Overview

`neuron-mcp` connects the
[Model Context Protocol](https://modelcontextprotocol.io) (MCP) ecosystem to neuron's
`ToolRegistry`. It provides:

- **MCP client** — connects to an MCP server (stdio child process, HTTP, or streamable HTTP),
  discovers its tools, and registers them into a `ToolRegistry` so any neuron operator can call them
- **MCP server** — exposes a `ToolRegistry` as an MCP server endpoint, making neuron tools
  accessible to any MCP-capable client

Backed by the [`rmcp`](https://crates.io/crates/rmcp) library.

## Usage

```toml
[dependencies]
neuron-mcp = "0.4"
neuron-tool = "0.4"
tokio = { version = "1", features = ["full"] }
```

### Consuming an MCP server's tools

```rust
use neuron_mcp::McpClientBridge;
use neuron_tool::ToolRegistry;

let bridge = McpClientBridge::from_child_process("uvx", &["mcp-server-fetch"]).await?;
let mut registry = ToolRegistry::new();
bridge.register_into(&mut registry).await?;
// registry now contains all tools from the MCP server
```

### Exposing neuron tools as an MCP server

```rust
use neuron_mcp::McpServerBridge;

let bridge = McpServerBridge::new(registry);
bridge.serve_stdio().await?;
```

## Part of the neuron workspace

[neuron](https://github.com/secbear/neuron) is a composable async agentic AI framework for Rust.
See the [book](https://secbear.github.io/neuron) for architecture and guides.
