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
