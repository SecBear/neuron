//! OpenAI API client struct and builder.

use agent_types::{CompletionRequest, CompletionResponse, Provider, ProviderError, StreamHandle};

use crate::error::{map_http_status, map_reqwest_error};
use crate::mapping::{from_api_response, to_api_request};
use crate::streaming::stream_completion;

/// Default model used when none is specified on the request.
const DEFAULT_MODEL: &str = "gpt-4o";

/// Default OpenAI API base URL.
const DEFAULT_BASE_URL: &str = "https://api.openai.com";

/// Client for the OpenAI Chat Completions API.
///
/// Implements [`Provider`] for use anywhere a provider is accepted.
///
/// # Example
///
/// ```no_run
/// use agent_provider_openai::OpenAi;
///
/// let client = OpenAi::new("sk-...")
///     .model("gpt-4o")
///     .base_url("https://api.openai.com")
///     .organization("org-...");
/// ```
pub struct OpenAi {
    /// OpenAI API key.
    pub(crate) api_key: String,
    /// Default model identifier used when the request does not specify one.
    pub(crate) model: String,
    /// API base URL (override for testing, proxies, or Azure).
    pub(crate) base_url: String,
    /// Optional organization ID for multi-org accounts.
    pub(crate) organization: Option<String>,
    /// Shared HTTP client.
    pub(crate) client: reqwest::Client,
}

impl OpenAi {
    /// Create a new client with the given API key and sensible defaults.
    ///
    /// Default model: `gpt-4o`.
    /// Default base URL: `https://api.openai.com`.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.into(),
            base_url: DEFAULT_BASE_URL.into(),
            organization: None,
            client: reqwest::Client::new(),
        }
    }

    /// Override the default model.
    ///
    /// This is used when [`CompletionRequest::model`] is empty.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Override the API base URL.
    ///
    /// Useful for testing with a local mock server, an API proxy, or Azure OpenAI.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the OpenAI organization ID.
    ///
    /// Sent as the `OpenAI-Organization` header on every request.
    pub fn organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }

    /// Build the chat completions endpoint URL.
    pub(crate) fn completions_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }

}

impl Provider for OpenAi {
    /// Send a completion request to the OpenAI Chat Completions API.
    ///
    /// Maps the [`CompletionRequest`] to OpenAI's JSON format, sends it with
    /// the required headers, and maps the response back to [`CompletionResponse`].
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        let url = self.completions_url();
        let api_key = self.api_key.clone();
        let default_model = self.model.clone();
        let organization = self.organization.clone();
        let http_client = self.client.clone();

        async move {
            let mut body = to_api_request(&request, &default_model);
            body["stream"] = serde_json::Value::Bool(false);

            tracing::debug!(url = %url, model = %body["model"], "sending completion request");

            let mut req = http_client
                .post(&url)
                .header("authorization", format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .json(&body);

            if let Some(org) = &organization {
                req = req.header("openai-organization", org);
            }

            let response = req.send().await.map_err(map_reqwest_error)?;

            let status = response.status();
            let response_text = response.text().await.map_err(map_reqwest_error)?;

            if !status.is_success() {
                return Err(map_http_status(status, &response_text));
            }

            let json: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
                ProviderError::InvalidRequest(format!("invalid JSON response: {e}"))
            })?;

            from_api_response(&json)
        }
    }

    /// Send a streaming completion request to the OpenAI Chat Completions API.
    ///
    /// Returns a [`StreamHandle`] whose receiver emits [`StreamEvent`]s as the
    /// model generates content.
    fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        let url = self.completions_url();
        let api_key = self.api_key.clone();
        let default_model = self.model.clone();
        let organization = self.organization.clone();
        let http_client = self.client.clone();

        async move {
            let mut body = to_api_request(&request, &default_model);
            body["stream"] = serde_json::Value::Bool(true);
            // Request usage stats in the stream
            body["stream_options"] = serde_json::json!({ "include_usage": true });

            tracing::debug!(url = %url, model = %body["model"], "sending streaming completion request");

            let mut req = http_client
                .post(&url)
                .header("authorization", format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .json(&body);

            if let Some(org) = &organization {
                req = req.header("openai-organization", org);
            }

            let response = req.send().await.map_err(map_reqwest_error)?;

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
        let client = OpenAi::new("test-key");
        assert_eq!(client.model, DEFAULT_MODEL);
    }

    #[test]
    fn default_base_url_is_set() {
        let client = OpenAi::new("test-key");
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn builder_overrides_model() {
        let client = OpenAi::new("test-key").model("gpt-4o-mini");
        assert_eq!(client.model, "gpt-4o-mini");
    }

    #[test]
    fn builder_overrides_base_url() {
        let client = OpenAi::new("test-key").base_url("http://localhost:9999");
        assert_eq!(client.base_url, "http://localhost:9999");
    }

    #[test]
    fn builder_sets_organization() {
        let client = OpenAi::new("test-key").organization("org-abc123");
        assert_eq!(client.organization, Some("org-abc123".to_string()));
    }

    #[test]
    fn organization_default_is_none() {
        let client = OpenAi::new("test-key");
        assert!(client.organization.is_none());
    }

    #[test]
    fn completions_url_includes_path() {
        let client = OpenAi::new("test-key").base_url("http://localhost:9999");
        assert_eq!(
            client.completions_url(),
            "http://localhost:9999/v1/chat/completions"
        );
    }

    #[test]
    fn api_key_is_stored() {
        let client = OpenAi::new("sk-test-key");
        assert_eq!(client.api_key, "sk-test-key");
    }
}
