//! Ollama API client struct and builder.

/// Default model used when none is specified on the request.
const DEFAULT_MODEL: &str = "llama3.2";

/// Default Ollama API base URL.
const DEFAULT_BASE_URL: &str = "http://localhost:11434";

/// Client for the Ollama Chat API.
///
/// Implements [`Provider`] for use anywhere a provider is accepted.
///
/// # Example
///
/// ```no_run
/// use agent_provider_ollama::Ollama;
///
/// let client = Ollama::new()
///     .model("llama3.2")
///     .base_url("http://localhost:11434");
/// ```
pub struct Ollama {
    /// Default model identifier used when the request does not specify one.
    pub(crate) model: String,
    /// API base URL (override for testing or remote Ollama instances).
    pub(crate) base_url: String,
    /// Optional keep_alive duration string (e.g. "5m", "0" to unload).
    pub(crate) keep_alive: Option<String>,
    /// Shared HTTP client.
    pub(crate) client: reqwest::Client,
}

impl Ollama {
    /// Create a new client with sensible defaults.
    ///
    /// Default model: `llama3.2`.
    /// Default base URL: `http://localhost:11434`.
    /// No authentication required (Ollama is local).
    pub fn new() -> Self {
        Self {
            model: DEFAULT_MODEL.into(),
            base_url: DEFAULT_BASE_URL.into(),
            keep_alive: None,
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
    /// Useful for testing with a local mock server or a remote Ollama instance.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the keep_alive duration for model memory residency.
    ///
    /// Examples: `"5m"` (keep for 5 minutes), `"0"` (unload immediately after request).
    /// When not set, Ollama uses its server default.
    pub fn keep_alive(mut self, duration: impl Into<String>) -> Self {
        self.keep_alive = Some(duration.into());
        self
    }

    /// Build the chat endpoint URL.
    pub(crate) fn chat_url(&self) -> String {
        format!("{}/api/chat", self.base_url)
    }
}

impl Default for Ollama {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_set() {
        let client = Ollama::new();
        assert_eq!(client.model, DEFAULT_MODEL);
    }

    #[test]
    fn default_base_url_is_set() {
        let client = Ollama::new();
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn builder_overrides_model() {
        let client = Ollama::new().model("mistral");
        assert_eq!(client.model, "mistral");
    }

    #[test]
    fn builder_overrides_base_url() {
        let client = Ollama::new().base_url("http://remote:11434");
        assert_eq!(client.base_url, "http://remote:11434");
    }

    #[test]
    fn builder_sets_keep_alive() {
        let client = Ollama::new().keep_alive("5m");
        assert_eq!(client.keep_alive, Some("5m".to_string()));
    }

    #[test]
    fn keep_alive_defaults_to_none() {
        let client = Ollama::new();
        assert!(client.keep_alive.is_none());
    }

    #[test]
    fn chat_url_includes_path() {
        let client = Ollama::new().base_url("http://localhost:9999");
        assert_eq!(client.chat_url(), "http://localhost:9999/api/chat");
    }

    #[test]
    fn default_impl_matches_new() {
        let client = Ollama::default();
        assert_eq!(client.model, DEFAULT_MODEL);
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
    }
}
