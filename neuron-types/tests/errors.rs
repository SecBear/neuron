use neuron_types::*;
use std::time::Duration;

#[test]
fn provider_error_display() {
    let err = ProviderError::RateLimit {
        retry_after: Some(Duration::from_secs(30)),
    };
    assert!(err.to_string().contains("rate limited"));
}

#[test]
fn provider_error_is_retryable() {
    assert!(
        ProviderError::Network(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "timeout"
        )))
        .is_retryable()
    );
    assert!(ProviderError::RateLimit { retry_after: None }.is_retryable());
    assert!(ProviderError::Timeout(Duration::from_secs(5)).is_retryable());
    assert!(ProviderError::ServiceUnavailable("down".into()).is_retryable());
    assert!(!ProviderError::Authentication("bad key".into()).is_retryable());
    assert!(!ProviderError::InvalidRequest("bad".into()).is_retryable());
}

#[test]
fn loop_error_from_provider() {
    let pe = ProviderError::Authentication("bad".into());
    let le: LoopError = pe.into();
    assert!(le.to_string().contains("provider error"));
}

#[test]
fn tool_error_display() {
    let err = ToolError::NotFound("read_file".into());
    assert!(err.to_string().contains("read_file"));
}

#[test]
fn context_error_from_provider() {
    let pe = ProviderError::Network(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "fail",
    )));
    let ce: ContextError = pe.into();
    assert!(ce.to_string().contains("provider error"));
}

#[test]
fn durable_error_display() {
    let err = DurableError::ActivityFailed("timeout".into());
    assert!(err.to_string().contains("timeout"));
}

#[test]
fn mcp_error_display() {
    let err = McpError::Connection("refused".into());
    assert!(err.to_string().contains("refused"));
}

// --- ProviderError: display for all variants ---

#[test]
fn provider_error_network_display() {
    let err = ProviderError::Network(Box::new(std::io::Error::new(
        std::io::ErrorKind::ConnectionReset,
        "connection reset",
    )));
    assert!(err.to_string().contains("network error"));
}

#[test]
fn provider_error_rate_limit_no_retry_after_display() {
    let err = ProviderError::RateLimit { retry_after: None };
    let display = err.to_string();
    assert!(display.contains("rate limited"));
    assert!(display.contains("None"));
}

#[test]
fn provider_error_model_loading_display() {
    let err = ProviderError::ModelLoading("initializing weights".into());
    assert!(err.to_string().contains("model loading"));
    assert!(err.to_string().contains("initializing weights"));
}

#[test]
fn provider_error_model_loading_is_retryable() {
    assert!(ProviderError::ModelLoading("cold start".into()).is_retryable());
}

#[test]
fn provider_error_timeout_display() {
    let err = ProviderError::Timeout(Duration::from_secs(30));
    assert!(err.to_string().contains("timeout after"));
    assert!(err.to_string().contains("30"));
}

#[test]
fn provider_error_service_unavailable_display() {
    let err = ProviderError::ServiceUnavailable("overloaded".into());
    assert!(err.to_string().contains("service unavailable"));
    assert!(err.to_string().contains("overloaded"));
}

#[test]
fn provider_error_authentication_display() {
    let err = ProviderError::Authentication("invalid API key".into());
    assert!(err.to_string().contains("authentication failed"));
    assert!(err.to_string().contains("invalid API key"));
}

#[test]
fn provider_error_invalid_request_display() {
    let err = ProviderError::InvalidRequest("missing model field".into());
    assert!(err.to_string().contains("invalid request"));
    assert!(err.to_string().contains("missing model field"));
}

#[test]
fn provider_error_model_not_found_display() {
    let err = ProviderError::ModelNotFound("gpt-99".into());
    let display = err.to_string();
    assert!(display.contains("model not found"));
    assert!(display.contains("gpt-99"));
    assert!(!err.is_retryable());
}

#[test]
fn provider_error_insufficient_resources_display() {
    let err = ProviderError::InsufficientResources("quota exceeded".into());
    let display = err.to_string();
    assert!(display.contains("insufficient resources"));
    assert!(display.contains("quota exceeded"));
    assert!(!err.is_retryable());
}

#[test]
fn provider_error_stream_error_display() {
    let err = ProviderError::StreamError("unexpected EOF".into());
    let display = err.to_string();
    assert!(display.contains("stream error"));
    assert!(display.contains("unexpected EOF"));
    assert!(!err.is_retryable());
}

#[test]
fn provider_error_other_display() {
    let err = ProviderError::Other(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "unknown failure",
    )));
    let display = err.to_string();
    assert!(display.contains("unknown failure"));
    assert!(!err.is_retryable());
}

// --- ToolError: display for all variants ---

#[test]
fn tool_error_invalid_input_display() {
    let err = ToolError::InvalidInput("missing 'path' field".into());
    let display = err.to_string();
    assert!(display.contains("invalid input"));
    assert!(display.contains("missing 'path' field"));
}

#[test]
fn tool_error_execution_failed_display() {
    let err = ToolError::ExecutionFailed(Box::new(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found",
    )));
    let display = err.to_string();
    assert!(display.contains("execution failed"));
}

#[test]
fn tool_error_permission_denied_display() {
    let err = ToolError::PermissionDenied("bash commands not allowed".into());
    let display = err.to_string();
    assert!(display.contains("permission denied"));
    assert!(display.contains("bash commands not allowed"));
}

#[test]
fn tool_error_cancelled_display() {
    let err = ToolError::Cancelled;
    assert_eq!(err.to_string(), "cancelled");
}

#[test]
fn tool_error_model_retry_display() {
    let err = ToolError::ModelRetry("try using absolute path instead".into());
    let display = err.to_string();
    assert!(display.contains("model retry requested"));
    assert!(display.contains("try using absolute path instead"));
}

// --- ContextError: display for all variants ---

#[test]
fn context_error_compaction_failed_display() {
    let err = ContextError::CompactionFailed("summary too long".into());
    let display = err.to_string();
    assert!(display.contains("compaction failed"));
    assert!(display.contains("summary too long"));
}

// --- LoopError: display and conversions for all variants ---

#[test]
fn loop_error_from_tool_error() {
    let te = ToolError::NotFound("my_tool".into());
    let le: LoopError = te.into();
    let display = le.to_string();
    assert!(display.contains("tool error"));
    assert!(display.contains("my_tool"));
}

#[test]
fn loop_error_from_context_error() {
    let ce = ContextError::CompactionFailed("out of memory".into());
    let le: LoopError = ce.into();
    let display = le.to_string();
    assert!(display.contains("context error"));
}

#[test]
fn loop_error_max_turns_display() {
    let err = LoopError::MaxTurns(50);
    let display = err.to_string();
    assert!(display.contains("max turns reached"));
    assert!(display.contains("50"));
}

#[test]
fn loop_error_hook_terminated_display() {
    let err = LoopError::HookTerminated("safety guardrail triggered".into());
    let display = err.to_string();
    assert!(display.contains("terminated by hook"));
    assert!(display.contains("safety guardrail triggered"));
}

#[test]
fn loop_error_cancelled_display() {
    let err = LoopError::Cancelled;
    assert_eq!(err.to_string(), "cancelled");
}

// --- DurableError: display for all variants ---

#[test]
fn durable_error_cancelled_display() {
    let err = DurableError::Cancelled;
    assert_eq!(err.to_string(), "workflow cancelled");
}

#[test]
fn durable_error_signal_timeout_display() {
    let err = DurableError::SignalTimeout;
    assert_eq!(err.to_string(), "signal timeout");
}

#[test]
fn durable_error_continue_as_new_display() {
    let err = DurableError::ContinueAsNew("history too large".into());
    let display = err.to_string();
    assert!(display.contains("continue as new"));
    assert!(display.contains("history too large"));
}

#[test]
fn durable_error_other_display() {
    let err = DurableError::Other(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "temporal unavailable",
    )));
    let display = err.to_string();
    assert!(display.contains("temporal unavailable"));
}

// --- McpError: display for all variants ---

#[test]
fn mcp_error_initialization_display() {
    let err = McpError::Initialization("handshake failed".into());
    let display = err.to_string();
    assert!(display.contains("initialization failed"));
    assert!(display.contains("handshake failed"));
}

#[test]
fn mcp_error_tool_call_display() {
    let err = McpError::ToolCall("tool timed out".into());
    let display = err.to_string();
    assert!(display.contains("tool call failed"));
    assert!(display.contains("tool timed out"));
}

#[test]
fn mcp_error_transport_display() {
    let err = McpError::Transport("broken pipe".into());
    let display = err.to_string();
    assert!(display.contains("transport error"));
    assert!(display.contains("broken pipe"));
}

#[test]
fn mcp_error_other_display() {
    let err = McpError::Other(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "unknown MCP failure",
    )));
    let display = err.to_string();
    assert!(display.contains("unknown MCP failure"));
}

// --- HookError: display for all variants ---

#[test]
fn hook_error_failed_display() {
    let err = HookError::Failed("metrics backend unreachable".into());
    let display = err.to_string();
    assert!(display.contains("hook failed"));
    assert!(display.contains("metrics backend unreachable"));
}

#[test]
fn hook_error_other_display() {
    let err = HookError::Other(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "serialization failure",
    )));
    let display = err.to_string();
    assert!(display.contains("serialization failure"));
}

// --- EmbeddingError: display and is_retryable for all variants ---

#[test]
fn embedding_error_authentication_display() {
    let err = EmbeddingError::Authentication("invalid key".into());
    let display = err.to_string();
    assert!(display.contains("authentication failed"));
    assert!(!err.is_retryable());
}

#[test]
fn embedding_error_rate_limit_display() {
    let err = EmbeddingError::RateLimit {
        retry_after: Some(Duration::from_secs(60)),
    };
    let display = err.to_string();
    assert!(display.contains("rate limited"));
    assert!(err.is_retryable());
}

#[test]
fn embedding_error_rate_limit_no_retry_after() {
    let err = EmbeddingError::RateLimit { retry_after: None };
    assert!(err.is_retryable());
}

#[test]
fn embedding_error_invalid_request_display() {
    let err = EmbeddingError::InvalidRequest("empty input".into());
    let display = err.to_string();
    assert!(display.contains("invalid request"));
    assert!(!err.is_retryable());
}

#[test]
fn embedding_error_network_display() {
    let err = EmbeddingError::Network(Box::new(std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        "connection refused",
    )));
    let display = err.to_string();
    assert!(display.contains("network error"));
    assert!(err.is_retryable());
}

#[test]
fn embedding_error_other_display() {
    let err = EmbeddingError::Other(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "unknown embedding error",
    )));
    let display = err.to_string();
    assert!(display.contains("unknown embedding error"));
    assert!(!err.is_retryable());
}

// --- StorageError: display for all variants ---

#[test]
fn storage_error_not_found_display() {
    let err = StorageError::NotFound("session-abc-123".into());
    let display = err.to_string();
    assert!(display.contains("not found"));
    assert!(display.contains("session-abc-123"));
}

#[test]
fn storage_error_serialization_display() {
    let err = StorageError::Serialization("invalid JSON".into());
    let display = err.to_string();
    assert!(display.contains("serialization error"));
    assert!(display.contains("invalid JSON"));
}

#[test]
fn storage_error_io_display() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let err: StorageError = io_err.into();
    let display = err.to_string();
    assert!(display.contains("io error"));
}

#[test]
fn storage_error_other_display() {
    let err = StorageError::Other(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "database connection lost",
    )));
    let display = err.to_string();
    assert!(display.contains("database connection lost"));
}

// --- SandboxError: display for all variants ---

#[test]
fn sandbox_error_execution_failed_display() {
    let err = SandboxError::ExecutionFailed("process exited with code 1".into());
    let display = err.to_string();
    assert!(display.contains("execution failed"));
    assert!(display.contains("process exited with code 1"));
}

#[test]
fn sandbox_error_setup_failed_display() {
    let err = SandboxError::SetupFailed("container runtime unavailable".into());
    let display = err.to_string();
    assert!(display.contains("sandbox error"));
    assert!(display.contains("container runtime unavailable"));
}

#[test]
fn sandbox_error_other_display() {
    let err = SandboxError::Other(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "unknown sandbox issue",
    )));
    let display = err.to_string();
    assert!(display.contains("unknown sandbox issue"));
}
