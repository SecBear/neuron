//! Error conversion utilities for MCP operations.
//!
//! Re-exports [`McpError`] from `neuron-types` and provides conversion
//! functions from rmcp's error types. We cannot implement `From` directly
//! due to Rust's orphan rules (both types are foreign).

pub use neuron_types::McpError;

/// Convert an rmcp `ServiceError` into our `McpError`.
pub(crate) fn from_service_error(err: rmcp::ServiceError) -> McpError {
    McpError::Transport(err.to_string())
}

/// Convert an rmcp `ClientInitializeError` into our `McpError`.
pub(crate) fn from_client_init_error(err: rmcp::service::ClientInitializeError) -> McpError {
    McpError::Initialization(err.to_string())
}
