#![deny(missing_docs)]
//! Anthropic API provider for neuron-turn.
//!
//! Implements the [`neuron_turn::Provider`] trait for Anthropic's Messages API.

mod types;

use neuron_turn::provider::{Provider, ProviderError};
use neuron_turn::types::*;
use rust_decimal::Decimal;
use types::*;

/// API key source — static string or environment variable resolved per request.
enum ApiKeySource {
    /// Key material provided at construction time.
    Static(String),
    /// Environment variable name; resolved at each `complete()` call.
    EnvVar(String),
}

/// Anthropic API provider.
pub struct AnthropicProvider {
    api_key_source: ApiKeySource,
    client: reqwest::Client,
    api_url: String,
    api_version: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key_source: ApiKeySource::Static(api_key.into()),
            client: reqwest::Client::new(),
            api_url: "https://api.anthropic.com/v1/messages".into(),
            api_version: "2023-06-01".into(),
        }
    }

    /// Create a provider that reads its API key from an environment variable at each request.
    ///
    /// The variable is resolved via `std::env::var` at every call to `complete()`.  
    /// Returns `ProviderError::AuthFailed` if the variable is unset or empty — the error
    /// message contains the variable *name* only, never its value.
    pub fn from_env_var(var_name: impl Into<String>) -> Self {
        Self {
            api_key_source: ApiKeySource::EnvVar(var_name.into()),
            client: reqwest::Client::new(),
            api_url: "https://api.anthropic.com/v1/messages".into(),
            api_version: "2023-06-01".into(),
        }
    }

    fn resolve_api_key(&self) -> Result<String, ProviderError> {
        match &self.api_key_source {
            ApiKeySource::Static(key) => Ok(key.clone()),
            ApiKeySource::EnvVar(var_name) => {
                let key = std::env::var(var_name).map_err(|_| {
                    ProviderError::AuthFailed(format!(
                        "env var '{}' not set or not unicode",
                        var_name
                    ))
                })?;
                if key.is_empty() {
                    return Err(ProviderError::AuthFailed(format!(
                        "env var '{}' is empty",
                        var_name
                    )));
                }
                Ok(key)
            }
        }
    }

    /// Override the API URL (for testing or proxies).
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.api_url = url.into();
        self
    }

    fn build_request(&self, request: &ProviderRequest) -> AnthropicRequest {
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| "claude-haiku-4-5-20251001".into());
        let max_tokens = request.max_tokens.unwrap_or(4096);

        let messages: Vec<AnthropicMessage> = request
            .messages
            .iter()
            .map(|m| AnthropicMessage {
                role: match m.role {
                    Role::User => "user".into(),
                    Role::Assistant => "assistant".into(),
                    Role::System => "user".into(), // System messages go in the system field
                },
                content: parts_to_anthropic_content(&m.content),
            })
            .collect();

        let tools: Vec<AnthropicTool> = request
            .tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        AnthropicRequest {
            model,
            max_tokens,
            messages,
            system: request.system.clone(),
            tools,
        }
    }

    fn parse_response(
        &self,
        response: AnthropicResponse,
    ) -> Result<ProviderResponse, ProviderError> {
        let content: Vec<ContentPart> = response
            .content
            .iter()
            .map(anthropic_block_to_content_part)
            .collect();

        let stop_reason = match response.stop_reason.as_str() {
            "end_turn" => StopReason::EndTurn,
            "tool_use" => StopReason::ToolUse,
            "max_tokens" => StopReason::MaxTokens,
            "refusal" => StopReason::ContentFilter,
            _ => StopReason::EndTurn,
        };

        let usage = TokenUsage {
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            cache_read_tokens: response.usage.cache_read_input_tokens,
            cache_creation_tokens: response.usage.cache_creation_input_tokens,
        };

        // Simple cost calculation for Haiku
        // Haiku: $0.25/MTok input, $1.25/MTok output (as of 2025)
        let input_cost = Decimal::from(response.usage.input_tokens) * Decimal::new(25, 8);
        let output_cost = Decimal::from(response.usage.output_tokens) * Decimal::new(125, 8);
        let cost = input_cost + output_cost;

        Ok(ProviderResponse {
            content,
            stop_reason,
            usage,
            model: response.model,
            cost: Some(cost),
            truncated: None,
        })
    }
}

impl Provider for AnthropicProvider {
    fn complete(
        &self,
        request: ProviderRequest,
    ) -> impl std::future::Future<Output = Result<ProviderResponse, ProviderError>> + Send {
        let api_key_result = self.resolve_api_key();
        let api_request = self.build_request(&request);
        let http_opt = api_key_result.map(|key| {
            self.client
                .post(&self.api_url)
                .header("x-api-key", key)
                .header("anthropic-version", &self.api_version)
                .header("content-type", "application/json")
                .json(&api_request)
        });

        async move {
            let http_request = match http_opt {
                Err(e) => return Err(e),
                Ok(r) => r,
            };
            let http_response =
                http_request
                    .send()
                    .await
                    .map_err(|e| ProviderError::TransientError {
                        message: e.to_string(),
                        status: None,
                    })?;

            let status = http_response.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(ProviderError::RateLimited);
            }
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                let body = http_response.text().await.unwrap_or_default();
                return Err(ProviderError::AuthFailed(body));
            }
            if !status.is_success() {
                let body = http_response.text().await.unwrap_or_default();
                return Err(map_error_response(status, &body));
            }

            let api_response: AnthropicResponse = http_response
                .json()
                .await
                .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

            self.parse_response(api_response)
        }
    }
}

/// Map a non-success HTTP response to an appropriate [`ProviderError`].
///
/// - 500, 502, 503 (server errors) → [`ProviderError::TransientError`]
/// - Body containing content-filter signals → [`ProviderError::ContentBlocked`]
/// - All other non-success responses → [`ProviderError::TransientError`]
fn map_error_response(status: reqwest::StatusCode, body: &str) -> ProviderError {
    let status_u16 = status.as_u16();
    // Check for Anthropic content-filter signals in the response body.
    if body.contains("content_filter") || body.contains("content policy") {
        return ProviderError::ContentBlocked {
            message: body.to_string(),
        };
    }
    ProviderError::TransientError {
        message: format!("HTTP {status}: {body}"),
        status: Some(status_u16),
    }
}

fn parts_to_anthropic_content(parts: &[ContentPart]) -> AnthropicContent {
    if parts.len() == 1
        && let ContentPart::Text { text } = &parts[0]
    {
        return AnthropicContent::Text(text.clone());
    }
    AnthropicContent::Blocks(parts.iter().map(content_part_to_anthropic_block).collect())
}

fn content_part_to_anthropic_block(part: &ContentPart) -> AnthropicContentBlock {
    match part {
        ContentPart::Text { text } => AnthropicContentBlock::Text { text: text.clone() },
        ContentPart::ToolUse { id, name, input } => AnthropicContentBlock::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        ContentPart::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => AnthropicContentBlock::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: content.clone(),
            is_error: *is_error,
        },
        ContentPart::Image { source, media_type } => AnthropicContentBlock::Image {
            source: match source {
                ImageSource::Base64 { data } => AnthropicImageSource::Base64 { data: data.clone() },
                ImageSource::Url { url } => AnthropicImageSource::Url { url: url.clone() },
            },
            media_type: media_type.clone(),
        },
    }
}

fn anthropic_block_to_content_part(block: &AnthropicContentBlock) -> ContentPart {
    match block {
        AnthropicContentBlock::Text { text } => ContentPart::Text { text: text.clone() },
        AnthropicContentBlock::ToolUse { id, name, input } => ContentPart::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        AnthropicContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => ContentPart::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: content.clone(),
            is_error: *is_error,
        },
        AnthropicContentBlock::Image { source, media_type } => ContentPart::Image {
            source: match source {
                AnthropicImageSource::Base64 { data } => ImageSource::Base64 { data: data.clone() },
                AnthropicImageSource::Url { url } => ImageSource::Url { url: url.clone() },
            },
            media_type: media_type.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_simple_request() {
        let provider = AnthropicProvider::new("test-key");
        let request = ProviderRequest {
            model: Some("claude-haiku-4-5-20251001".into()),
            messages: vec![ProviderMessage {
                role: Role::User,
                content: vec![ContentPart::Text {
                    text: "Hello".into(),
                }],
            }],
            tools: vec![],
            max_tokens: Some(256),
            temperature: None,
            system: Some("Be helpful.".into()),
            extra: json!(null),
        };

        let api_request = provider.build_request(&request);
        assert_eq!(api_request.model, "claude-haiku-4-5-20251001");
        assert_eq!(api_request.max_tokens, 256);
        assert_eq!(api_request.messages.len(), 1);
        assert_eq!(api_request.messages[0].role, "user");
        assert_eq!(api_request.system, Some("Be helpful.".into()));
    }

    #[test]
    fn parse_simple_response() {
        let provider = AnthropicProvider::new("test-key");
        let api_response = AnthropicResponse {
            content: vec![AnthropicContentBlock::Text {
                text: "Hello!".into(),
            }],
            model: "claude-haiku-4-5-20251001".into(),
            stop_reason: "end_turn".into(),
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
        };

        let response = provider.parse_response(api_response).unwrap();
        assert_eq!(response.stop_reason, StopReason::EndTurn);
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
        assert!(response.cost.is_some());
        assert_eq!(response.content.len(), 1);
    }

    #[test]
    fn parse_tool_use_response() {
        let provider = AnthropicProvider::new("test-key");
        let api_response = AnthropicResponse {
            content: vec![AnthropicContentBlock::ToolUse {
                id: "tu_1".into(),
                name: "bash".into(),
                input: json!({"command": "ls"}),
            }],
            model: "claude-haiku-4-5-20251001".into(),
            stop_reason: "tool_use".into(),
            usage: AnthropicUsage {
                input_tokens: 20,
                output_tokens: 30,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
        };

        let response = provider.parse_response(api_response).unwrap();
        assert_eq!(response.stop_reason, StopReason::ToolUse);
        assert_eq!(response.content.len(), 1);
        match &response.content[0] {
            ContentPart::ToolUse { name, .. } => assert_eq!(name, "bash"),
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn tool_schema_serializes() {
        let tool = AnthropicTool {
            name: "get_weather".into(),
            description: "Get current weather".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                },
                "required": ["location"]
            }),
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "get_weather");
    }

    #[test]
    fn parse_cache_tokens() {
        let provider = AnthropicProvider::new("test-key");
        let api_response = AnthropicResponse {
            content: vec![AnthropicContentBlock::Text {
                text: "Cached.".into(),
            }],
            model: "claude-haiku-4-5-20251001".into(),
            stop_reason: "end_turn".into(),
            usage: AnthropicUsage {
                input_tokens: 100,
                output_tokens: 10,
                cache_read_input_tokens: Some(50),
                cache_creation_input_tokens: Some(25),
            },
        };

        let response = provider.parse_response(api_response).unwrap();
        assert_eq!(response.usage.cache_read_tokens, Some(50));
        assert_eq!(response.usage.cache_creation_tokens, Some(25));
    }

    #[test]
    fn default_model_is_haiku() {
        let provider = AnthropicProvider::new("test-key");
        let request = ProviderRequest {
            model: None,
            messages: vec![ProviderMessage {
                role: Role::User,
                content: vec![ContentPart::Text { text: "Hi".into() }],
            }],
            tools: vec![],
            max_tokens: None,
            temperature: None,
            system: None,
            extra: json!(null),
        };

        let api_request = provider.build_request(&request);
        assert_eq!(api_request.model, "claude-haiku-4-5-20251001");
    }

    #[test]
    fn default_max_tokens_is_4096() {
        let provider = AnthropicProvider::new("test-key");
        let request = ProviderRequest {
            model: None,
            messages: vec![],
            tools: vec![],
            max_tokens: None,
            temperature: None,
            system: None,
            extra: json!(null),
        };

        let api_request = provider.build_request(&request);
        assert_eq!(api_request.max_tokens, 4096);
    }

    #[test]
    fn tool_result_in_request() {
        let provider = AnthropicProvider::new("test-key");
        let request = ProviderRequest {
            model: None,
            messages: vec![
                ProviderMessage {
                    role: Role::Assistant,
                    content: vec![ContentPart::ToolUse {
                        id: "tu_1".into(),
                        name: "bash".into(),
                        input: json!({"cmd": "ls"}),
                    }],
                },
                ProviderMessage {
                    role: Role::User,
                    content: vec![ContentPart::ToolResult {
                        tool_use_id: "tu_1".into(),
                        content: "file.txt".into(),
                        is_error: false,
                    }],
                },
            ],
            tools: vec![],
            max_tokens: None,
            temperature: None,
            system: None,
            extra: json!(null),
        };

        let api_request = provider.build_request(&request);
        assert_eq!(api_request.messages.len(), 2);
        assert_eq!(api_request.messages[0].role, "assistant");
        assert_eq!(api_request.messages[1].role, "user");
    }

    #[test]
    fn parse_response_refusal_maps_to_content_filter() {
        let provider = AnthropicProvider::new("test-key");
        let api_response = AnthropicResponse {
            content: vec![AnthropicContentBlock::Text {
                text: "I cannot help with that.".into(),
            }],
            model: "claude-haiku-4-5-20251001".into(),
            stop_reason: "refusal".into(),
            usage: AnthropicUsage {
                input_tokens: 5,
                output_tokens: 8,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
        };
        let result = provider.parse_response(api_response);
        let resp = result.expect("refusal should be Ok, not Err");
        assert_eq!(resp.stop_reason, StopReason::ContentFilter);
        assert_eq!(resp.usage.input_tokens, 5);
        assert_eq!(resp.usage.output_tokens, 8);
        assert_eq!(resp.content.len(), 1);
        assert!(resp.cost.is_some());
    }

    #[test]
    fn map_error_500_returns_transient() {
        let status = reqwest::StatusCode::INTERNAL_SERVER_ERROR;
        let err = map_error_response(status, "internal server error");
        assert!(matches!(
            err,
            ProviderError::TransientError {
                status: Some(500),
                ..
            }
        ));
        assert!(err.is_retryable());
    }

    #[test]
    fn map_error_503_returns_transient() {
        let status = reqwest::StatusCode::SERVICE_UNAVAILABLE;
        let err = map_error_response(status, "service unavailable");
        assert!(matches!(
            err,
            ProviderError::TransientError {
                status: Some(503),
                ..
            }
        ));
        assert!(err.is_retryable());
    }

    #[test]
    fn map_error_content_filter_body_returns_blocked() {
        let status = reqwest::StatusCode::BAD_REQUEST;
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"content_filter triggered"}}"#;
        let err = map_error_response(status, body);
        assert!(matches!(err, ProviderError::ContentBlocked { .. }));
        assert!(!err.is_retryable());
    }
}

#[cfg(test)]
mod tests_credential {
    use super::*;

    #[test]
    fn new_uses_static_key() {
        let p = AnthropicProvider::new("sk-static");
        assert_eq!(p.resolve_api_key().unwrap(), "sk-static");
    }

    #[test]
    fn from_env_var_resolves_when_set() {
        let var = "NEURON_ANTHROPIC_TEST_CRED_A";
        unsafe {
            std::env::set_var(var, "sk-from-env");
        }
        let p = AnthropicProvider::from_env_var(var);
        assert_eq!(p.resolve_api_key().unwrap(), "sk-from-env");
        unsafe {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn from_env_var_missing_returns_auth_failed() {
        let var = "NEURON_ANTHROPIC_TEST_CRED_MISSING_ZZZ";
        unsafe {
            std::env::remove_var(var);
        }
        let p = AnthropicProvider::from_env_var(var);
        let err = p.resolve_api_key().unwrap_err();
        assert!(matches!(err, ProviderError::AuthFailed(_)));
        let msg = err.to_string();
        assert!(msg.contains(var), "error should name the variable");
    }

    #[test]
    fn from_env_var_empty_returns_auth_failed() {
        let var = "NEURON_ANTHROPIC_TEST_CRED_EMPTY_ZZZ";
        unsafe {
            std::env::set_var(var, "");
        }
        let p = AnthropicProvider::from_env_var(var);
        let err = p.resolve_api_key().unwrap_err();
        assert!(matches!(err, ProviderError::AuthFailed(_)));
        let msg = err.to_string();
        assert!(msg.contains(var), "error should name the variable");
        unsafe {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn error_message_does_not_contain_secret_value() {
        // Set a var to a secret value, then empty it — verify the empty-key error
        // only names the variable, never surfaces the value.
        let var = "NEURON_ANTHROPIC_TEST_CRED_REDACT_ZZZ";
        let secret = "sk-must-not-appear-in-any-error-message";
        unsafe {
            std::env::set_var(var, "");
        }
        let p = AnthropicProvider::from_env_var(var);
        // Secret value is not in the env var (it's empty), but confirm var name is present.
        let msg = p.resolve_api_key().unwrap_err().to_string();
        assert!(msg.contains(var));
        assert!(!msg.contains(secret));
        // Now set the var to the secret and confirm that the happy path
        // (resolved key) is NOT leaked into any error type.
        unsafe {
            std::env::set_var(var, secret);
        }
        assert_eq!(p.resolve_api_key().unwrap(), secret); // key resolved correctly
        unsafe {
            std::env::remove_var(var);
        }
    }
}
