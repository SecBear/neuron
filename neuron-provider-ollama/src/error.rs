//! Internal error helpers for mapping HTTP/reqwest errors to [`ProviderError`].

use std::time::Duration;

use neuron_types::ProviderError;

/// Map an HTTP status code (from the Ollama API) to a [`ProviderError`].
///
/// Reference: <https://github.com/ollama/ollama/blob/main/docs/api.md>
pub(crate) fn map_http_status(status: reqwest::StatusCode, body: &str) -> ProviderError {
    match status.as_u16() {
        404 => ProviderError::ModelNotFound(body.to_string()),
        400 => ProviderError::InvalidRequest(body.to_string()),
        500..=599 => ProviderError::ServiceUnavailable(body.to_string()),
        _ => ProviderError::InvalidRequest(format!("HTTP {status}: {body}")),
    }
}

/// Map a [`reqwest::Error`] to a [`ProviderError`].
pub(crate) fn map_reqwest_error(err: reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        ProviderError::Timeout(Duration::from_secs(30))
    } else {
        ProviderError::Network(Box::new(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_404_maps_to_model_not_found() {
        let err = map_http_status(reqwest::StatusCode::NOT_FOUND, "model 'foo' not found");
        assert!(matches!(err, ProviderError::ModelNotFound(msg) if msg == "model 'foo' not found"));
    }

    #[test]
    fn status_400_maps_to_invalid_request() {
        let err = map_http_status(reqwest::StatusCode::BAD_REQUEST, "bad body");
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg == "bad body"));
    }

    #[test]
    fn status_500_maps_to_service_unavailable() {
        let err = map_http_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "internal error");
        assert!(matches!(err, ProviderError::ServiceUnavailable(msg) if msg == "internal error"));
    }

    #[test]
    fn status_502_maps_to_service_unavailable() {
        let err = map_http_status(reqwest::StatusCode::BAD_GATEWAY, "bad gateway");
        assert!(matches!(err, ProviderError::ServiceUnavailable(msg) if msg == "bad gateway"));
    }

    #[test]
    fn status_503_maps_to_service_unavailable() {
        let err = map_http_status(reqwest::StatusCode::SERVICE_UNAVAILABLE, "unavailable");
        assert!(matches!(err, ProviderError::ServiceUnavailable(msg) if msg == "unavailable"));
    }

    #[test]
    fn status_599_maps_to_service_unavailable() {
        let status = reqwest::StatusCode::from_u16(599).expect("valid status");
        let err = map_http_status(status, "edge case");
        assert!(matches!(err, ProviderError::ServiceUnavailable(msg) if msg == "edge case"));
    }

    #[test]
    fn unknown_status_maps_to_invalid_request_with_status() {
        let err = map_http_status(reqwest::StatusCode::FORBIDDEN, "forbidden");
        match err {
            ProviderError::InvalidRequest(msg) => {
                assert!(msg.contains("403"), "expected status in message: {msg}");
                assert!(msg.contains("forbidden"), "expected body in message: {msg}");
            }
            other => panic!("expected InvalidRequest, got: {other:?}"),
        }
    }

    #[test]
    fn status_429_maps_to_invalid_request_fallthrough() {
        let err = map_http_status(reqwest::StatusCode::TOO_MANY_REQUESTS, "rate limited");
        match err {
            ProviderError::InvalidRequest(msg) => {
                assert!(msg.contains("429"), "expected status in message: {msg}");
                assert!(
                    msg.contains("rate limited"),
                    "expected body in message: {msg}"
                );
            }
            other => panic!("expected InvalidRequest, got: {other:?}"),
        }
    }

    #[test]
    fn status_5xx_errors_are_retryable() {
        let err = map_http_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "internal error");
        assert!(err.is_retryable());
    }

    #[test]
    fn status_400_errors_are_not_retryable() {
        let err = map_http_status(reqwest::StatusCode::BAD_REQUEST, "bad body");
        assert!(!err.is_retryable());
    }

    #[test]
    fn status_404_errors_are_not_retryable() {
        let err = map_http_status(reqwest::StatusCode::NOT_FOUND, "not found");
        assert!(!err.is_retryable());
    }

    #[test]
    fn empty_body_preserved_in_error() {
        let err = map_http_status(reqwest::StatusCode::BAD_REQUEST, "");
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg.is_empty()));
    }
}
