//! Integration tests for the Anthropic provider using wiremock.

use neuron_provider_anthropic::Anthropic;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, ProviderError, Role};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn minimal_request() -> CompletionRequest {
    CompletionRequest {
        model: String::new(),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello".into())],
        }],
        system: None,
        tools: vec![],
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    }
}

fn success_response_body() -> serde_json::Value {
    serde_json::json!({
        "id": "msg_01XFDUDYJgAACzvnptvVoYEL",
        "type": "message",
        "role": "assistant",
        "model": "claude-sonnet-4-20250514",
        "content": [{ "type": "text", "text": "Hello! How can I help you today?" }],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 12,
            "output_tokens": 10
        }
    })
}

#[tokio::test]
async fn complete_sends_correct_headers() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "test-api-key"))
        .and(header("anthropic-version", "2023-06-01"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Anthropic::new("test-api-key").base_url(mock_server.uri());

    let result = provider.complete(minimal_request()).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());

    let resp = result.unwrap();
    assert_eq!(resp.id, "msg_01XFDUDYJgAACzvnptvVoYEL");
    assert_eq!(resp.usage.input_tokens, 12);
    assert_eq!(resp.usage.output_tokens, 10);
}

#[tokio::test]
async fn complete_parses_text_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .mount(&mock_server)
        .await;

    let provider = Anthropic::new("key").base_url(mock_server.uri());
    let resp = provider.complete(minimal_request()).await.unwrap();

    assert!(
        matches!(&resp.message.content[0], ContentBlock::Text(t) if t == "Hello! How can I help you today?")
    );
}

#[tokio::test]
async fn complete_returns_rate_limit_error_on_429() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "type": "error",
            "error": { "type": "rate_limit_error", "message": "Too many requests" }
        })))
        .mount(&mock_server)
        .await;

    let provider = Anthropic::new("key").base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::RateLimit { .. }),
        "expected RateLimit, got: {err:?}"
    );
    assert!(err.is_retryable());
}

#[tokio::test]
async fn complete_returns_authentication_error_on_401() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "type": "error",
            "error": { "type": "authentication_error", "message": "Invalid API key" }
        })))
        .mount(&mock_server)
        .await;

    let provider = Anthropic::new("bad-key").base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::Authentication(_)),
        "expected Authentication, got: {err:?}"
    );
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn complete_returns_service_unavailable_on_529() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_json(serde_json::json!({
            "type": "error",
            "error": { "type": "overloaded_error", "message": "Overloaded" }
        })))
        .mount(&mock_server)
        .await;

    let provider = Anthropic::new("key").base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::ServiceUnavailable(_)),
        "expected ServiceUnavailable, got: {err:?}"
    );
    assert!(err.is_retryable());
}

#[tokio::test]
async fn complete_returns_invalid_request_on_400() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "type": "error",
            "error": { "type": "invalid_request_error", "message": "Bad request" }
        })))
        .mount(&mock_server)
        .await;

    let provider = Anthropic::new("key").base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::InvalidRequest(_)),
        "expected InvalidRequest, got: {err:?}"
    );
}

#[tokio::test]
async fn complete_returns_model_not_found_on_404() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "type": "error",
            "error": { "type": "not_found_error", "message": "Model not found" }
        })))
        .mount(&mock_server)
        .await;

    let provider = Anthropic::new("key").base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::ModelNotFound(_)),
        "expected ModelNotFound, got: {err:?}"
    );
}
