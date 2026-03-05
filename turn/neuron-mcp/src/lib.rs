#![deny(missing_docs)]
//! MCP client and server bridging MCP tools with neuron ToolRegistry.
//!
//! Two independent components:
//!
//! - [`McpClient`] connects to an MCP server, discovers its tools, resources,
//!   and prompts, wrapping each as appropriate types for use in neuron.
//! - [`McpServer`] wraps a [`ToolRegistry`](neuron_tool::ToolRegistry) and
//!   exposes its tools (and optionally state resources and prompt templates)
//!   via the MCP protocol over stdio.

pub mod client;
pub mod error;
pub mod server;

pub use client::{McpClient, McpPromptWrapper, McpResourceWrapper, TOOL_COUNT_WARN_THRESHOLD};
pub use error::McpError;
pub use server::McpServer;
