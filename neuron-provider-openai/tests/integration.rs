//! Integration tests for the OpenAI provider using wiremock.

use std::collections::HashMap;

use futures::StreamExt;
use neuron_provider_openai::OpenAi;
use neuron_types::{
    CompletionRequest, ContentBlock, EmbeddingError, EmbeddingProvider, EmbeddingRequest,
    ImageSource, Message, Provider, ProviderError, Role, StreamEvent,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn minimal_request() -> CompletionRequest {
    CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello".into())],
        }],
        ..Default::default()
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

// --- Embedding integration tests ---

fn embedding_success_response() -> serde_json::Value {
    serde_json::json!({
        "object": "list",
        "data": [
            {
                "object": "embedding",
                "embedding": [0.0023064255, -0.009327292, 0.015797347],
                "index": 0
            }
        ],
        "model": "text-embedding-3-small",
        "usage": {
            "prompt_tokens": 5,
            "total_tokens": 5
        }
    })
}

#[tokio::test]
async fn embed_sends_correct_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(header("authorization", "Bearer test-embed-key"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(embedding_success_response()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("test-embed-key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        model: "text-embedding-3-small".to_string(),
        input: vec!["Hello world".to_string()],
        dimensions: None,
        extra: HashMap::new(),
    };

    let result = provider.embed(request).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());

    let resp = result.unwrap();
    assert_eq!(resp.embeddings.len(), 1);
    assert_eq!(resp.embeddings[0].len(), 3);
    assert_eq!(resp.model, "text-embedding-3-small");
    assert_eq!(resp.usage.prompt_tokens, 5);
    assert_eq!(resp.usage.total_tokens, 5);
}

#[tokio::test]
async fn embed_sends_correct_body_fields() {
    let mock_server = MockServer::start().await;

    // Capture the request body to verify its contents
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "model": "text-embedding-3-small",
            "input": ["text one", "text two"],
            "encoding_format": "float"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": [
                { "object": "embedding", "embedding": [0.1, 0.2], "index": 0 },
                { "object": "embedding", "embedding": [0.3, 0.4], "index": 1 }
            ],
            "model": "text-embedding-3-small",
            "usage": { "prompt_tokens": 8, "total_tokens": 8 }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        model: "text-embedding-3-small".to_string(),
        input: vec!["text one".to_string(), "text two".to_string()],
        dimensions: None,
        extra: HashMap::new(),
    };

    let resp = provider.embed(request).await.unwrap();
    assert_eq!(resp.embeddings.len(), 2);
}

#[tokio::test]
async fn embed_returns_auth_error_on_401() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "type": "authentication_error",
                "message": "Invalid API key"
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("bad-key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        input: vec!["test".to_string()],
        ..Default::default()
    };

    let err = provider.embed(request).await.unwrap_err();
    assert!(
        matches!(err, EmbeddingError::Authentication(_)),
        "expected Authentication, got: {err:?}"
    );
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn embed_with_dimensions() {
    let mock_server = MockServer::start().await;

    // Use a custom matcher to verify the dimensions field is present
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "dimensions": 256
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": [
                { "object": "embedding", "embedding": [0.1, 0.2], "index": 0 }
            ],
            "model": "text-embedding-3-small",
            "usage": { "prompt_tokens": 3, "total_tokens": 3 }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        model: "text-embedding-3-small".to_string(),
        input: vec!["hello".to_string()],
        dimensions: Some(256),
        extra: HashMap::new(),
    };

    let resp = provider.embed(request).await.unwrap();
    assert_eq!(resp.embeddings.len(), 1);
    assert_eq!(resp.embeddings[0].len(), 2);
}

#[tokio::test]
async fn embed_rate_limit_on_429() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": {
                "type": "rate_limit_error",
                "message": "Rate limit exceeded. Please retry after 30 seconds."
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        input: vec!["test".to_string()],
        ..Default::default()
    };

    let err = provider.embed(request).await.unwrap_err();
    assert!(
        matches!(err, EmbeddingError::RateLimit { .. }),
        "expected RateLimit, got: {err:?}"
    );
    assert!(err.is_retryable());

    // Verify retry_after is parsed
    if let EmbeddingError::RateLimit { retry_after } = err {
        assert_eq!(retry_after, Some(std::time::Duration::from_secs(30)));
    }
}

#[tokio::test]
async fn embed_uses_default_model_when_empty() {
    let mock_server = MockServer::start().await;

    // Verify that when no model is specified, text-embedding-3-small is used
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "model": "text-embedding-3-small"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(embedding_success_response()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        model: String::new(),
        input: vec!["hello".to_string()],
        dimensions: None,
        extra: HashMap::new(),
    };

    let result = provider.embed(request).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
}

// --- Streaming integration tests ---

/// Helper to build an SSE body string from a slice of data payloads.
fn sse_body(data_lines: &[&str]) -> String {
    let mut body = String::new();
    for line in data_lines {
        body.push_str(&format!("data: {line}\r\n\r\n"));
    }
    body
}

#[tokio::test]
async fn stream_complete_text_response() {
    let mock_server = MockServer::start().await;

    let sse = sse_body(&[
        r#"{"id":"chatcmpl-stream1","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
        r#"{"id":"chatcmpl-stream1","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#,
        r#"{"id":"chatcmpl-stream1","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}"#,
        r#"{"id":"chatcmpl-stream1","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12}}"#,
        "[DONE]",
    ]);

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer stream-key"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("stream-key").base_url(mock_server.uri());
    let handle = provider.complete_stream(minimal_request()).await.unwrap();

    let events: Vec<StreamEvent> = handle.receiver.collect().await;

    // Should have text deltas
    let text: String = events
        .iter()
        .filter_map(|e| {
            if let StreamEvent::TextDelta(t) = e {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(text, "Hello world");

    // Should have usage
    let has_usage = events.iter().any(
        |e| matches!(e, StreamEvent::Usage(u) if u.input_tokens == 10 && u.output_tokens == 2),
    );
    assert!(has_usage, "expected Usage event in stream");

    // Should have MessageComplete with assembled text
    let has_complete = events.iter().any(|e| {
        matches!(e, StreamEvent::MessageComplete(msg) if {
            matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello world")
        })
    });
    assert!(has_complete, "expected MessageComplete event");
}

#[tokio::test]
async fn stream_tool_calls_response() {
    let mock_server = MockServer::start().await;

    let sse = sse_body(&[
        r#"{"id":"chatcmpl-tc","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"call_stream1","type":"function","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]}"#,
        r#"{"id":"chatcmpl-tc","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"city\""}}]},"finish_reason":null}]}"#,
        r#"{"id":"chatcmpl-tc","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"NYC\"}"}}]},"finish_reason":null}]}"#,
        r#"{"id":"chatcmpl-tc","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":15,"completion_tokens":8,"total_tokens":23}}"#,
        "[DONE]",
    ]);

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let handle = provider.complete_stream(minimal_request()).await.unwrap();

    let events: Vec<StreamEvent> = handle.receiver.collect().await;

    // Should have ToolUseStart
    let has_start = events.iter().any(|e| {
        matches!(e, StreamEvent::ToolUseStart { id, name }
            if id == "call_stream1" && name == "get_weather")
    });
    assert!(has_start, "expected ToolUseStart event");

    // Should have ToolUseInputDelta chunks
    let input_json: String = events
        .iter()
        .filter_map(|e| {
            if let StreamEvent::ToolUseInputDelta { delta, .. } = e {
                Some(delta.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(input_json, r#"{"city":"NYC"}"#);

    // Should have ToolUseEnd
    let has_end = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolUseEnd { id } if id == "call_stream1"));
    assert!(has_end, "expected ToolUseEnd event");

    // Should have MessageComplete with assembled tool use
    let has_complete = events.iter().any(|e| {
        matches!(e, StreamEvent::MessageComplete(msg) if {
            matches!(&msg.content[0], ContentBlock::ToolUse { id, name, input }
                if id == "call_stream1" && name == "get_weather" && input["city"] == "NYC")
        })
    });
    assert!(has_complete, "expected MessageComplete with tool use");
}

#[tokio::test]
async fn stream_returns_error_on_non_success_status() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": {
                "type": "rate_limit_error",
                "message": "Rate limit exceeded. Please retry after 10 seconds."
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let err = provider
        .complete_stream(minimal_request())
        .await
        .unwrap_err();
    assert!(
        matches!(err, ProviderError::RateLimit { .. }),
        "expected RateLimit, got: {err:?}"
    );
}

// --- Error status code edge cases ---

#[tokio::test]
async fn complete_returns_authentication_error_on_403() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "error": {
                "type": "permission_error",
                "message": "You do not have access to this resource"
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::Authentication(_)),
        "expected Authentication on 403, got: {err:?}"
    );
    assert!(!err.is_retryable());
}

// --- Image content in completion request ---

#[tokio::test]
async fn complete_with_image_content_sends_correct_body() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![
                ContentBlock::Text("What's in this image?".into()),
                ContentBlock::Image {
                    source: ImageSource::Url {
                        url: "https://example.com/photo.jpg".into(),
                    },
                },
            ],
        }],
        ..Default::default()
    };

    let result = provider.complete(request).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
}

// --- Embedding edge case tests ---

#[tokio::test]
async fn embed_extra_fields_merged_into_body() {
    let mock_server = MockServer::start().await;

    // Verify that extra fields like "user" are merged into the request body
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "user": "test-user-123"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(embedding_success_response()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let mut extra = HashMap::new();
    extra.insert(
        "user".to_string(),
        serde_json::Value::String("test-user-123".into()),
    );
    let request = EmbeddingRequest {
        model: "text-embedding-3-small".to_string(),
        input: vec!["hello".to_string()],
        dimensions: None,
        extra,
    };

    let result = provider.embed(request).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
}

#[tokio::test]
async fn embed_returns_authentication_error_on_403() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "error": {
                "type": "permission_error",
                "message": "Forbidden"
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        input: vec!["test".to_string()],
        ..Default::default()
    };

    let err = provider.embed(request).await.unwrap_err();
    assert!(
        matches!(err, EmbeddingError::Authentication(_)),
        "expected Authentication on 403, got: {err:?}"
    );
}

#[tokio::test]
async fn embed_returns_server_error_on_500() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(serde_json::json!({ "error": { "message": "Internal error" } })),
        )
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        input: vec!["test".to_string()],
        ..Default::default()
    };

    let err = provider.embed(request).await.unwrap_err();
    assert!(
        matches!(err, EmbeddingError::Other(_)),
        "expected Other on 500, got: {err:?}"
    );
}

#[tokio::test]
async fn embed_returns_invalid_request_on_404() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {
                "type": "not_found",
                "message": "Model not found"
            }
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let request = EmbeddingRequest {
        input: vec!["test".to_string()],
        ..Default::default()
    };

    let err = provider.embed(request).await.unwrap_err();
    assert!(
        matches!(err, EmbeddingError::InvalidRequest(_)),
        "expected InvalidRequest on 404, got: {err:?}"
    );
}

// --- Streaming finish_reason edge cases ---

#[tokio::test]
async fn stream_with_empty_content_emits_no_message_complete() {
    let mock_server = MockServer::start().await;

    // Stream that only has a stop event with no actual content
    let sse = sse_body(&[
        r#"{"id":"chatcmpl-empty","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
        r#"{"id":"chatcmpl-empty","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ]);

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let handle = provider.complete_stream(minimal_request()).await.unwrap();

    let events: Vec<StreamEvent> = handle.receiver.collect().await;

    // Should NOT have MessageComplete because no actual content was streamed
    let has_complete = events
        .iter()
        .any(|e| matches!(e, StreamEvent::MessageComplete(_)));
    assert!(
        !has_complete,
        "expected no MessageComplete for empty content stream"
    );
}

#[tokio::test]
async fn stream_with_usage_and_cached_tokens() {
    let mock_server = MockServer::start().await;

    let sse = sse_body(&[
        r#"{"id":"chatcmpl-cached","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":"Ok"},"finish_reason":null}]}"#,
        r#"{"id":"chatcmpl-cached","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":1,"total_tokens":101,"prompt_tokens_details":{"cached_tokens":80},"completion_tokens_details":{"reasoning_tokens":0}}}"#,
        "[DONE]",
    ]);

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .mount(&mock_server)
        .await;

    let provider = OpenAi::new("key").base_url(mock_server.uri());
    let handle = provider.complete_stream(minimal_request()).await.unwrap();

    let events: Vec<StreamEvent> = handle.receiver.collect().await;

    let usage_event = events.iter().find_map(|e| {
        if let StreamEvent::Usage(u) = e {
            Some(u)
        } else {
            None
        }
    });
    assert!(usage_event.is_some(), "expected Usage event");
    let usage = usage_event.unwrap();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 1);
    assert_eq!(usage.cache_read_tokens, Some(80));
    assert_eq!(usage.reasoning_tokens, Some(0));
}
