//! Internal error helpers for mapping HTTP/reqwest errors to [`ProviderError`].

use std::time::Duration;

use neuron_types::ProviderError;

/// Map an HTTP status code (from the OpenAI API) to a [`ProviderError`].
///
/// Reference: <https://platform.openai.com/docs/guides/error-codes>
pub(crate) fn map_http_status(status: reqwest::StatusCode, body: &str) -> ProviderError {
    match status.as_u16() {
        401 => ProviderError::Authentication(body.to_string()),
        400 => ProviderError::InvalidRequest(body.to_string()),
        404 => ProviderError::ModelNotFound(body.to_string()),
        403 => ProviderError::Authentication(body.to_string()),
        // 429 may include a Retry-After header; we parse from the body as best-effort
        429 => ProviderError::RateLimit {
            retry_after: parse_retry_after(body),
        },
        500 | 502 | 503 => ProviderError::ServiceUnavailable(body.to_string()),
        _ => ProviderError::InvalidRequest(format!("HTTP {status}: {body}")),
    }
}

/// Attempt to parse a retry delay from an OpenAI error body.
///
/// OpenAI sometimes includes "Please retry after X seconds" in the error message.
/// This is a best-effort parse; returns `None` if no delay can be extracted.
fn parse_retry_after(body: &str) -> Option<Duration> {
    // Look for "retry after <N>" pattern in the body text
    let lower = body.to_lowercase();
    if let Some(idx) = lower.find("retry after ") {
        let after = &lower[idx + 12..];
        // Try to parse the number of seconds
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(secs) = num_str.parse::<u64>() {
            return Some(Duration::from_secs(secs));
        }
    }
    None
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
    fn map_401_to_authentication() {
        let err = map_http_status(reqwest::StatusCode::UNAUTHORIZED, "Invalid API key");
        assert!(matches!(err, ProviderError::Authentication(_)));
    }

    #[test]
    fn map_400_to_invalid_request() {
        let err = map_http_status(reqwest::StatusCode::BAD_REQUEST, "Bad request");
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    #[test]
    fn map_404_to_model_not_found() {
        let err = map_http_status(reqwest::StatusCode::NOT_FOUND, "Model not found");
        assert!(matches!(err, ProviderError::ModelNotFound(_)));
    }

    #[test]
    fn map_429_to_rate_limit() {
        let err = map_http_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded",
        );
        assert!(matches!(err, ProviderError::RateLimit { .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn map_429_with_retry_after() {
        let err = map_http_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "Please retry after 60 seconds",
        );
        match err {
            ProviderError::RateLimit { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(60)));
            }
            _ => panic!("expected RateLimit"),
        }
    }

    #[test]
    fn map_500_to_service_unavailable() {
        let err = map_http_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "Server error");
        assert!(matches!(err, ProviderError::ServiceUnavailable(_)));
        assert!(err.is_retryable());
    }

    #[test]
    fn map_502_to_service_unavailable() {
        let err = map_http_status(reqwest::StatusCode::BAD_GATEWAY, "Bad gateway");
        assert!(matches!(err, ProviderError::ServiceUnavailable(_)));
    }

    #[test]
    fn map_503_to_service_unavailable() {
        let err = map_http_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "Service unavailable",
        );
        assert!(matches!(err, ProviderError::ServiceUnavailable(_)));
    }

    #[test]
    fn map_unknown_status_to_invalid_request() {
        let err = map_http_status(reqwest::StatusCode::IM_A_TEAPOT, "I'm a teapot");
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    #[test]
    fn parse_retry_after_extracts_seconds() {
        let result = parse_retry_after("Please retry after 30 seconds");
        assert_eq!(result, Some(Duration::from_secs(30)));
    }

    #[test]
    fn parse_retry_after_returns_none_when_not_found() {
        let result = parse_retry_after("Generic error message");
        assert_eq!(result, None);
    }
}
