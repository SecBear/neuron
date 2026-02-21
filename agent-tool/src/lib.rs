//! Tool registry and middleware pipeline for rust-agent-blocks.
//!
//! This crate provides:
//! - [`ToolRegistry`] — register tools and execute them by name
//! - [`ToolMiddleware`] — composable middleware chain (like axum's `from_fn`)
//! - Built-in middleware for schema validation, permissions, output formatting

pub mod middleware;
pub mod registry;
pub mod builtin;

pub use middleware::*;
pub use registry::*;
pub use builtin::*;

#[cfg(feature = "macros")]
pub use agent_tool_macros::agent_tool;
