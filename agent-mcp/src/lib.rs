//! MCP (Model Context Protocol) integration for rust-agent-blocks.
//!
//! This crate wraps [`rmcp`] to provide:
//! - [`McpClient`] — connect to MCP servers via stdio or HTTP
//! - [`McpToolBridge`] — bridges MCP tools into the [`ToolDyn`] trait for use in [`ToolRegistry`]
//! - [`McpServer`] — exposes a [`ToolRegistry`] as an MCP server
//!
//! The key value-add is bridging MCP tools to our `Tool`/`ToolDyn` trait system
//! and providing ergonomic lifecycle management.

pub mod client;
pub mod bridge;
pub mod server;
pub mod types;
pub mod error;

pub use client::*;
pub use bridge::*;
pub use server::*;
pub use types::*;
pub use error::*;
