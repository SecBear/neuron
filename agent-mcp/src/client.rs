//! MCP client: connect to MCP servers via stdio or HTTP.

/// Placeholder for MCP client -- implemented in subsequent tasks.
pub struct McpClient {
    pub(crate) _private: (),
}

/// Configuration for an MCP stdio connection.
pub struct StdioConfig {
    /// The command to spawn.
    pub command: String,
    /// Arguments to pass to the command.
    pub args: Vec<String>,
    /// Environment variables to set.
    pub env: Vec<(String, String)>,
}

/// Configuration for an MCP HTTP connection.
pub struct HttpConfig {
    /// The server URL.
    pub url: String,
    /// Optional authorization header value.
    pub auth_header: Option<String>,
    /// Additional HTTP headers.
    pub headers: Vec<(String, String)>,
}
