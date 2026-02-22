//! Internal error helpers for mapping HTTP/reqwest errors to [`ProviderError`].

use std::time::Duration;

use neuron_types::ProviderError;

/// Map an HTTP status code (from the Anthropic API) to a [`ProviderError`].
///
/// Reference: <https://docs.anthropic.com/en/api/errors>
pub(crate) fn map_http_status(
    status: reqwest::StatusCode,
    body: &str,
) -> ProviderError {
    match status.as_u16() {
        401 => ProviderError::Authentication(body.to_string()),
        400 => ProviderError::InvalidRequest(body.to_string()),
        404 => ProviderError::ModelNotFound(body.to_string()),
        // 429 may include a Retry-After header; we parse from body as best-effort
        429 => ProviderError::RateLimit { retry_after: parse_retry_after(body) },
        // 529 is Anthropic's overloaded status
        529 => ProviderError::ServiceUnavailable(body.to_string()),
        500..=528 | 530..=599 => ProviderError::ServiceUnavailable(body.to_string()),
        _ => ProviderError::InvalidRequest(format!("HTTP {status}: {body}")),
    }
}

/// Attempt to parse a retry delay in seconds from an Anthropic error body.
///
/// Anthropic returns `{"error": {"type": "rate_limit_error", ...}}` but does
/// not include a retry delay in the body — that comes in the `Retry-After`
/// header, which we don't have here. Returns `None` always for now; callers
/// with header access can construct `RateLimit` directly.
fn parse_retry_after(_body: &str) -> Option<Duration> {
    None
}

/// Map a [`reqwest::Error`] to a [`ProviderError`].
pub(crate) fn map_reqwest_error(err: reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        // Use a generic 30-second duration since we don't track the configured timeout here
        ProviderError::Timeout(Duration::from_secs(30))
    } else {
        ProviderError::Network(Box::new(err))
    }
}
