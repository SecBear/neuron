//! Error types for all neuron crates.

use std::time::Duration;

/// Errors from LLM provider operations.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    // Retryable errors
    /// Network-level error (connection reset, DNS failure, etc.).
    #[error("network error: {0}")]
    Network(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// Rate limited by the provider.
    #[error("rate limited, retry after {retry_after:?}")]
    RateLimit {
        /// Suggested retry delay, if provided by the API.
        retry_after: Option<Duration>,
    },
    /// Model is still loading (cold start).
    #[error("model loading: {0}")]
    ModelLoading(String),
    /// Request timed out.
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    /// Provider service is temporarily unavailable.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    // Terminal errors
    /// Authentication/authorization failure.
    #[error("authentication failed: {0}")]
    Authentication(String),
    /// Malformed or invalid request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// Requested model does not exist.
    #[error("model not found: {0}")]
    ModelNotFound(String),
    /// Quota or resource limit exceeded.
    #[error("insufficient resources: {0}")]
    InsufficientResources(String),

    // Catch-all
    /// Error during streaming.
    #[error("stream error: {0}")]
    StreamError(String),
    /// Any other provider error.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl ProviderError {
    /// Whether this error is likely transient and the request can be retried.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_)
                | Self::RateLimit { .. }
                | Self::ModelLoading(_)
                | Self::Timeout(_)
                | Self::ServiceUnavailable(_)
        )
    }
}

/// Errors from tool operations.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Tool not found in registry.
    #[error("tool not found: {0}")]
    NotFound(String),
    /// Invalid input for the tool.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Tool execution failed.
    #[error("execution failed: {0}")]
    ExecutionFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// Permission denied for this tool call.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// Tool execution was cancelled.
    #[error("cancelled")]
    Cancelled,
    /// Tool requests the model to retry with the given hint.
    ///
    /// The hint is returned to the model as an error tool result so it can
    /// self-correct and call the tool again with adjusted arguments.
    #[error("model retry requested: {0}")]
    ModelRetry(String),
}

/// Errors from context management operations.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// Compaction strategy failed.
    #[error("compaction failed: {0}")]
    CompactionFailed(String),
    /// Provider error during summarization.
    #[error("provider error during summarization: {0}")]
    Provider(#[from] ProviderError),
}

/// Errors from the agentic loop.
#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    /// Provider call failed.
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    /// Tool execution failed.
    #[error("tool error: {0}")]
    Tool(#[from] ToolError),
    /// Context management failed.
    #[error("context error: {0}")]
    Context(#[from] ContextError),
    /// Loop exceeded the configured turn limit.
    #[error("max turns reached ({0})")]
    MaxTurns(usize),
    /// An observability hook terminated the loop.
    #[error("terminated by hook: {0}")]
    HookTerminated(String),
    /// The loop was cancelled via the cancellation token.
    #[error("cancelled")]
    Cancelled,
    /// A usage limit was exceeded (token budget, request limit, or tool call limit).
    #[error("usage limit exceeded: {0}")]
    UsageLimitExceeded(String),
}

/// Errors from durable execution operations.
#[derive(Debug, thiserror::Error)]
pub enum DurableError {
    /// An activity (LLM call or tool execution) failed.
    #[error("activity failed: {0}")]
    ActivityFailed(String),
    /// The workflow was cancelled.
    #[error("workflow cancelled")]
    Cancelled,
    /// Timed out waiting for a signal.
    #[error("signal timeout")]
    SignalTimeout,
    /// Continue-as-new was requested.
    #[error("continue as new: {0}")]
    ContinueAsNew(String),
    /// Any other durable execution error.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Errors from MCP operations.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// Failed to connect to MCP server.
    #[error("connection failed: {0}")]
    Connection(String),
    /// MCP initialization handshake failed.
    #[error("initialization failed: {0}")]
    Initialization(String),
    /// MCP tool call failed.
    #[error("tool call failed: {0}")]
    ToolCall(String),
    /// Transport-level error.
    #[error("transport error: {0}")]
    Transport(String),
    /// Any other MCP error.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Errors from observability hooks.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// Hook execution failed.
    #[error("hook failed: {0}")]
    Failed(String),
    /// Any other hook error.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Errors from embedding provider operations.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    /// Authentication/authorization failure.
    #[error("authentication failed: {0}")]
    Authentication(String),
    /// Rate limited by the provider.
    #[error("rate limited, retry after {retry_after:?}")]
    RateLimit {
        /// Suggested retry delay, if provided by the API.
        retry_after: Option<std::time::Duration>,
    },
    /// Malformed or invalid request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// Network-level error (connection reset, DNS failure, etc.).
    #[error("network error: {0}")]
    Network(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// Any other embedding error.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl EmbeddingError {
    /// Whether this error is likely transient and the request can be retried.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimit { .. } | Self::Network(_))
    }
}

/// Errors from session storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Session not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Serialization/deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// I/O error during storage operation.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Any other storage error.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Errors from sandbox operations.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// Tool execution failed within the sandbox.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    /// Sandbox setup or teardown failed.
    #[error("sandbox error: {0}")]
    SetupFailed(String),
    /// Any other sandbox error.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}
