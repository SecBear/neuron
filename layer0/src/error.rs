//! Error types for each protocol.

use thiserror::Error;

/// Turn execution errors.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum TurnError {
    /// An error from the model/LLM provider.
    #[error("model error: {0}")]
    Model(String),

    /// An error during tool execution.
    #[error("tool error in {tool}: {message}")]
    Tool {
        /// Name of the tool that failed.
        tool: String,
        /// Error message.
        message: String,
    },

    /// Context assembly failed before the model call.
    #[error("context assembly failed: {0}")]
    ContextAssembly(String),

    /// The turn failed but retrying might succeed.
    /// The orchestrator's retry policy decides.
    #[error("retryable: {0}")]
    Retryable(String),

    /// The turn failed and retrying won't help.
    /// Budget exceeded, invalid input, safety refusal.
    #[error("non-retryable: {0}")]
    NonRetryable(String),

    /// Catch-all. Include context.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Orchestration errors.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum OrchError {
    /// The requested agent was not found.
    #[error("agent not found: {0}")]
    AgentNotFound(String),

    /// The requested workflow was not found.
    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),

    /// Dispatching a turn failed.
    #[error("dispatch failed: {0}")]
    DispatchFailed(String),

    /// Signal delivery failed.
    #[error("signal delivery failed: {0}")]
    SignalFailed(String),

    /// A turn error propagated through orchestration.
    #[error("turn error: {0}")]
    TurnError(#[from] TurnError),

    /// Catch-all.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// State errors.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum StateError {
    /// Key not found in the given scope.
    #[error("not found: {scope}/{key}")]
    NotFound {
        /// The scope that was searched.
        scope: String,
        /// The key that was not found.
        key: String,
    },

    /// A write operation failed.
    #[error("write failed: {0}")]
    WriteFailed(String),

    /// Serialization or deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Catch-all.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Environment errors.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum EnvError {
    /// Failed to provision the execution environment.
    #[error("provisioning failed: {0}")]
    ProvisionFailed(String),

    /// The isolation boundary was violated.
    #[error("isolation violation: {0}")]
    IsolationViolation(String),

    /// Credential injection failed.
    #[error("credential injection failed: {0}")]
    CredentialFailed(String),

    /// A resource limit was exceeded.
    #[error("resource limit exceeded: {0}")]
    ResourceExceeded(String),

    /// A turn error propagated through the environment.
    #[error("turn error: {0}")]
    TurnError(#[from] TurnError),

    /// Catch-all.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Hook errors. These are logged but do NOT halt the turn
/// (use HookAction::Halt to halt).
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum HookError {
    /// The hook execution failed.
    #[error("hook failed: {0}")]
    Failed(String),

    /// Catch-all.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}
