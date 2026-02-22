//! Integration tests for the OpenAI provider using wiremock.

use neuron_provider_openai::OpenAi;
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
        "id": "chatcmpl-abc123",
        "object": "chat.completion",
        "model": "gpt-4o-2024-08-06",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you today?"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 10,
            "total_tokens": 22
        }
    })
}

#[tokio::test]
async fn complete_sends_correct_headers() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer test-api-key"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("test-api-key").base_url(mock_server.uri());

    let result = provider.complete(minimal_request()).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());

    let resp = result.unwrap();
    assert_eq!(resp.id, "chatcmpl-abc123");
    assert_eq!(resp.usage.input_tokens, 12);
    assert_eq!(resp.usage.output_tokens, 10);
}

#[tokio::test]
async fn complete_sends_organization_header() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer test-key"))
        .and(header("openai-organization", "org-abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("test-key")
        .base_url(mock_server.uri())
        .organization("org-abc123");

    let result = provider.complete(minimal_request()).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
}

#[tokio::test]
async fn complete_parses_text_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let resp = provider.complete(minimal_request()).await.unwrap();

    assert!(
        matches!(&resp.message.content[0], ContentBlock::Text(t) if t == "Hello! How can I help you today?")
    );
}

#[tokio::test]
async fn complete_parses_tool_call_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-tool",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{\"query\":\"rust\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 20, "completion_tokens": 15, "total_tokens": 35 }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let resp = provider.complete(minimal_request()).await.unwrap();

    assert_eq!(resp.stop_reason, neuron_types::StopReason::ToolUse);
    assert!(matches!(
        &resp.message.content[0],
        ContentBlock::ToolUse { id, name, input }
            if id == "call_abc123" && name == "search" && input["query"] == "rust"
    ));
}

#[tokio::test]
async fn complete_returns_rate_limit_error_on_429() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": {
                "type": "rate_limit_error",
                "message": "Rate limit exceeded. Please retry after 60 seconds."
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
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
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "type": "authentication_error",
                "message": "Invalid API key"
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("bad-key").base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::Authentication(_)),
        "expected Authentication, got: {err:?}"
    );
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn complete_returns_service_unavailable_on_500() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": {
                "type": "server_error",
                "message": "Internal server error"
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
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
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {
                "type": "invalid_request_error",
                "message": "Bad request"
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
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
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {
                "type": "not_found_error",
                "message": "Model not found"
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::ModelNotFound(_)),
        "expected ModelNotFound, got: {err:?}"
    );
}
