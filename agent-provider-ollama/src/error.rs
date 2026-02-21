//! Internal error helpers for mapping HTTP/reqwest errors to [`ProviderError`].

use std::time::Duration;

use agent_types::ProviderError;

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
