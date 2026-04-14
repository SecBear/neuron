#![deny(missing_docs)]
//! Tool interface for skelegent.

// TODO(v2): memory tools will be rebuilt as SyncOperator impls
// pub mod memory;
pub mod schema;

#[cfg(feature = "macros")]
pub use skg_tool_macro::skg_tool;

use thiserror::Error;

/// Errors from tool operations.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ToolError {
    /// The requested tool was not found in the registry.
    #[error("tool not found: {0}")]
    NotFound(String),

    /// Tool execution failed.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// The input provided to the tool was invalid.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Transient failure (network timeout, temporary unavailability).
    /// Callers should retry.
    #[error("transient error: {0}")]
    Transient(String),

    /// Rate limited by the upstream service.
    #[error("rate limited: {message}")]
    RateLimited {
        /// Suggested wait time before retry.
        retry_after: Option<std::time::Duration>,
        /// Human-readable message.
        message: String,
    },

    /// Catch-all for other errors.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl ToolError {
    /// Whether this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Transient(_) | Self::RateLimited { .. })
    }
}
