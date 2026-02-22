//! Anthropic API client struct and builder.

use neuron_types::{CompletionRequest, CompletionResponse, Provider, ProviderError, StreamHandle};

use crate::error::{map_http_status, map_reqwest_error};
use crate::mapping::{from_api_response, to_api_request};
use crate::streaming::stream_completion;

/// Default model used when none is specified on the request.
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";

/// Default Anthropic API base URL.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Client for the Anthropic Messages API.
///
/// Implements [`Provider`] for use anywhere a provider is accepted.
///
/// # Example
///
/// ```no_run
/// use neuron_provider_anthropic::Anthropic;
///
/// let client = Anthropic::new("sk-ant-...")
///     .model("claude-opus-4-5")
///     .base_url("https://api.anthropic.com");
/// ```
pub struct Anthropic {
    /// Anthropic API key (`ANTHROPIC_API_KEY`).
    pub(crate) api_key: String,
    /// Default model identifier used when the request does not specify one.
    pub(crate) model: String,
    /// API base URL (override for testing or proxies).
    pub(crate) base_url: String,
    /// Shared HTTP client.
    pub(crate) client: reqwest::Client,
}

impl Anthropic {
    /// Create a new client with the given API key and sensible defaults.
    ///
    /// Default model: `claude-sonnet-4-20250514`.
    /// Default base URL: `https://api.anthropic.com`.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.into(),
            base_url: DEFAULT_BASE_URL.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Override the default model.
    ///
    /// This is used when [`CompletionRequest::model`] is empty.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Override the API base URL.
    ///
    /// Useful for testing with a local mock server or an API proxy.
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Build the messages endpoint URL.
    pub(crate) fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url)
    }
}

impl Provider for Anthropic {
    /// Send a completion request to the Anthropic Messages API.
    ///
    /// Maps the [`CompletionRequest`] to Anthropic's JSON format, sends it with
    /// the required headers, and maps the response back to [`CompletionResponse`].
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        let url = self.messages_url();
        let api_key = self.api_key.clone();
        let default_model = self.model.clone();
        let http_client = self.client.clone();

        async move {
            let mut body = to_api_request(&request, &default_model);
            body["stream"] = serde_json::Value::Bool(false);

            tracing::debug!(url = %url, model = %body["model"], "sending completion request");

            let response = http_client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(map_reqwest_error)?;

            let status = response.status();
            let response_text = response.text().await.map_err(map_reqwest_error)?;

            if !status.is_success() {
                return Err(map_http_status(status, &response_text));
            }

            let json: serde_json::Value = serde_json::from_str(&response_text)
                .map_err(|e| ProviderError::InvalidRequest(format!("invalid JSON response: {e}")))?;

            from_api_response(&json)
        }
    }

    /// Send a streaming completion request to the Anthropic Messages API.
    ///
    /// Returns a [`StreamHandle`] whose receiver emits [`StreamEvent`]s as the
    /// model generates content.
    fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        let url = self.messages_url();
        let api_key = self.api_key.clone();
        let default_model = self.model.clone();
        let http_client = self.client.clone();

        async move {
            let mut body = to_api_request(&request, &default_model);
            body["stream"] = serde_json::Value::Bool(true);

            tracing::debug!(url = %url, model = %body["model"], "sending streaming completion request");

            let response = http_client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(map_reqwest_error)?;

            let status = response.status();
            if !status.is_success() {
                let body_text = response.text().await.map_err(map_reqwest_error)?;
                return Err(map_http_status(status, &body_text));
            }

            Ok(stream_completion(response))
        }
    }
}

// Required to satisfy the `use std::future::Future` in the trait impl bodies
use std::future::Future;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_set() {
        let client = Anthropic::new("test-key");
        assert_eq!(client.model, DEFAULT_MODEL);
    }

    #[test]
    fn default_base_url_is_set() {
        let client = Anthropic::new("test-key");
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn builder_overrides_model() {
        let client = Anthropic::new("test-key").model("claude-opus-4-5");
        assert_eq!(client.model, "claude-opus-4-5");
    }

    #[test]
    fn builder_overrides_base_url() {
        let client = Anthropic::new("test-key").base_url("http://localhost:9999");
        assert_eq!(client.base_url, "http://localhost:9999");
    }

    #[test]
    fn messages_url_includes_path() {
        let client = Anthropic::new("test-key").base_url("http://localhost:9999");
        assert_eq!(client.messages_url(), "http://localhost:9999/v1/messages");
    }

    #[test]
    fn api_key_is_stored() {
        let client = Anthropic::new("sk-ant-test");
        assert_eq!(client.api_key, "sk-ant-test");
    }
}
