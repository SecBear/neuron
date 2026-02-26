//! Error types for MCP operations.

/// Errors from MCP client and server operations.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// Connection to the MCP server or client failed.
    #[error("connection failed: {0}")]
    Connection(String),

    /// MCP protocol-level error.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Error related to tool operations.
    #[error("tool error: {0}")]
    Tool(String),

    /// Catch-all for other errors.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}
