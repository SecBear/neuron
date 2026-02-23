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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_service_error_transport_closed() {
        let err = rmcp::ServiceError::TransportClosed;
        let mcp_err = from_service_error(err);
        match &mcp_err {
            McpError::Transport(msg) => {
                assert!(msg.contains("Transport closed"), "got: {msg}");
            }
            other => panic!("expected Transport variant, got: {other:?}"),
        }
    }

    #[test]
    fn from_service_error_unexpected_response() {
        let err = rmcp::ServiceError::UnexpectedResponse;
        let mcp_err = from_service_error(err);
        match &mcp_err {
            McpError::Transport(msg) => {
                assert!(msg.contains("Unexpected response"), "got: {msg}");
            }
            other => panic!("expected Transport variant, got: {other:?}"),
        }
    }

    #[test]
    fn from_service_error_cancelled() {
        let err = rmcp::ServiceError::Cancelled {
            reason: Some("test reason".to_string()),
        };
        let mcp_err = from_service_error(err);
        match &mcp_err {
            McpError::Transport(msg) => {
                assert!(msg.contains("test reason"), "got: {msg}");
            }
            other => panic!("expected Transport variant, got: {other:?}"),
        }
    }

    #[test]
    fn from_service_error_cancelled_no_reason() {
        let err = rmcp::ServiceError::Cancelled { reason: None };
        let mcp_err = from_service_error(err);
        match &mcp_err {
            McpError::Transport(msg) => {
                assert!(msg.contains("cancelled"), "got: {msg}");
            }
            other => panic!("expected Transport variant, got: {other:?}"),
        }
    }

    #[test]
    fn from_service_error_timeout() {
        let err = rmcp::ServiceError::Timeout {
            timeout: std::time::Duration::from_secs(30),
        };
        let mcp_err = from_service_error(err);
        match &mcp_err {
            McpError::Transport(msg) => {
                assert!(msg.contains("timeout"), "got: {msg}");
            }
            other => panic!("expected Transport variant, got: {other:?}"),
        }
    }

    #[test]
    fn from_client_init_error_connection_closed() {
        let err = rmcp::service::ClientInitializeError::ConnectionClosed("peer gone".to_string());
        let mcp_err = from_client_init_error(err);
        match &mcp_err {
            McpError::Initialization(msg) => {
                assert!(msg.contains("peer gone"), "got: {msg}");
            }
            other => panic!("expected Initialization variant, got: {other:?}"),
        }
    }

    #[test]
    fn from_client_init_error_expected_init_response_none() {
        let err = rmcp::service::ClientInitializeError::ExpectedInitResponse(None);
        let mcp_err = from_client_init_error(err);
        match &mcp_err {
            McpError::Initialization(msg) => {
                assert!(msg.contains("initialized response"), "got: {msg}");
            }
            other => panic!("expected Initialization variant, got: {other:?}"),
        }
    }

    #[test]
    fn from_client_init_error_expected_init_result_none() {
        let err = rmcp::service::ClientInitializeError::ExpectedInitResult(None);
        let mcp_err = from_client_init_error(err);
        match &mcp_err {
            McpError::Initialization(msg) => {
                assert!(msg.contains("initialized result"), "got: {msg}");
            }
            other => panic!("expected Initialization variant, got: {other:?}"),
        }
    }

    #[test]
    fn mcp_error_display_transport() {
        let err = McpError::Transport("socket closed".to_string());
        assert_eq!(err.to_string(), "transport error: socket closed");
    }

    #[test]
    fn mcp_error_display_initialization() {
        let err = McpError::Initialization("handshake failed".to_string());
        assert_eq!(err.to_string(), "initialization failed: handshake failed");
    }

    #[test]
    fn mcp_error_display_connection() {
        let err = McpError::Connection("refused".to_string());
        assert_eq!(err.to_string(), "connection failed: refused");
    }

    #[test]
    fn mcp_error_display_tool_call() {
        let err = McpError::ToolCall("not found".to_string());
        assert_eq!(err.to_string(), "tool call failed: not found");
    }
}
