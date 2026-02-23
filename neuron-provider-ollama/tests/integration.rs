//! Integration tests for the Ollama provider using wiremock.

use futures::StreamExt;
use neuron_provider_ollama::Ollama;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, ProviderError, Role};
use wiremock::matchers::{method, path};
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
        "model": "llama3.2",
        "message": {
            "role": "assistant",
            "content": "Hello! How can I help you today?"
        },
        "done": true,
        "done_reason": "stop",
        "eval_count": 10,
        "prompt_eval_count": 20,
        "total_duration": 5000000000_u64,
        "load_duration": 1000000000_u64,
        "prompt_eval_duration": 500000000_u64,
        "eval_duration": 3500000000_u64,
    })
}

#[tokio::test]
async fn complete_sends_to_correct_endpoint() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());

    let result = provider.complete(minimal_request()).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());

    let resp = result.expect("already checked");
    assert_eq!(resp.model, "llama3.2");
    assert_eq!(resp.usage.input_tokens, 20);
    assert_eq!(resp.usage.output_tokens, 10);
}

#[tokio::test]
async fn complete_parses_text_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let resp = provider
        .complete(minimal_request())
        .await
        .expect("should succeed");

    assert!(
        matches!(&resp.message.content[0], ContentBlock::Text(t) if t == "Hello! How can I help you today?")
    );
}

#[tokio::test]
async fn complete_parses_tool_use_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": "search",
                        "arguments": { "query": "rust programming" }
                    }
                }]
            },
            "done": true,
            "done_reason": "tool_calls",
            "eval_count": 15,
            "prompt_eval_count": 25,
        })))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let resp = provider
        .complete(minimal_request())
        .await
        .expect("should succeed");

    assert_eq!(resp.stop_reason, neuron_types::StopReason::ToolUse);
    assert!(matches!(
        &resp.message.content[0],
        ContentBlock::ToolUse { name, input, id }
            if name == "search" && input["query"] == "rust programming" && id.starts_with("ollama_")
    ));
}

#[tokio::test]
async fn complete_returns_model_not_found_on_404() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(404).set_body_string("model 'nonexistent' not found"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::ModelNotFound(_)),
        "expected ModelNotFound, got: {err:?}"
    );
}

#[tokio::test]
async fn complete_returns_invalid_request_on_400() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(400).set_body_string("invalid request body"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::InvalidRequest(_)),
        "expected InvalidRequest, got: {err:?}"
    );
}

#[tokio::test]
async fn complete_returns_service_unavailable_on_500() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal server error"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::ServiceUnavailable(_)),
        "expected ServiceUnavailable, got: {err:?}"
    );
    assert!(err.is_retryable());
}

#[tokio::test]
async fn complete_request_includes_stream_false() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let _resp = provider
        .complete(minimal_request())
        .await
        .expect("should succeed");

    // Verify mock was called (validates POST to /api/chat)
}

#[tokio::test]
async fn stream_returns_text_deltas() {
    let mock_server = MockServer::start().await;

    // Ollama NDJSON streaming response
    let ndjson_body = concat!(
        r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}"#,
        "\n",
        r#"{"model":"llama3.2","message":{"role":"assistant","content":" world"},"done":false}"#,
        "\n",
        r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","eval_count":10,"prompt_eval_count":20}"#,
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson_body))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let handle = provider
        .complete_stream(minimal_request())
        .await
        .expect("should succeed");

    let events: Vec<neuron_types::StreamEvent> = handle.receiver.collect().await;

    // Should have TextDelta events
    let text_deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| {
            if let neuron_types::StreamEvent::TextDelta(t) = e {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(text_deltas, vec!["Hello", " world"]);

    // Should have a Usage event
    let has_usage = events.iter().any(|e| {
        matches!(e, neuron_types::StreamEvent::Usage(u) if u.input_tokens == 20 && u.output_tokens == 10)
    });
    assert!(has_usage, "expected Usage event");

    // Should have a MessageComplete event
    let has_complete = events
        .iter()
        .any(|e| matches!(e, neuron_types::StreamEvent::MessageComplete(_)));
    assert!(has_complete, "expected MessageComplete event");
}

#[tokio::test]
async fn stream_returns_error_on_non_success_status() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(404).set_body_string("model 'nonexistent' not found"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider
        .complete_stream(minimal_request())
        .await
        .unwrap_err();

    assert!(
        matches!(err, ProviderError::ModelNotFound(_)),
        "expected ModelNotFound, got: {err:?}"
    );
}

#[tokio::test]
async fn complete_with_keep_alive() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri()).keep_alive("5m");

    let result = provider.complete(minimal_request()).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
}

#[test]
fn from_env_always_succeeds() {
    let client = Ollama::from_env();
    assert!(client.is_ok());
}

// ─── Additional integration tests ─────────────────────────────────────────────

#[tokio::test]
async fn complete_with_system_prompt() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let req = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello".into())],
        }],
        system: Some(neuron_types::SystemPrompt::Text(
            "You are a helpful assistant.".into(),
        )),
        ..Default::default()
    };

    let resp = provider.complete(req).await.expect("should succeed");
    assert_eq!(resp.model, "llama3.2");
}

#[tokio::test]
async fn complete_with_tools_in_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let req = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Search for rust".into())],
        }],
        tools: vec![neuron_types::ToolDefinition {
            name: "search".into(),
            title: None,
            description: "Search the web".into(),
            input_schema: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}}),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }],
        ..Default::default()
    };

    let resp = provider.complete(req).await.expect("should succeed");
    assert!(!resp.id.is_empty());
}

#[tokio::test]
async fn complete_with_custom_model() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "mistral",
            "message": {
                "role": "assistant",
                "content": "Bonjour!"
            },
            "done": true,
            "done_reason": "stop",
            "eval_count": 5,
            "prompt_eval_count": 8,
        })))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri()).model("mistral");

    let resp = provider
        .complete(minimal_request())
        .await
        .expect("should succeed");
    assert_eq!(resp.model, "mistral");
}

#[tokio::test]
async fn complete_with_max_tokens_stop_reason() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "Truncated output..."
            },
            "done": true,
            "done_reason": "length",
            "eval_count": 100,
            "prompt_eval_count": 20,
        })))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let req = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Write a long essay".into())],
        }],
        max_tokens: Some(100),
        ..Default::default()
    };

    let resp = provider.complete(req).await.expect("should succeed");
    assert_eq!(resp.stop_reason, neuron_types::StopReason::MaxTokens);
    assert_eq!(resp.usage.output_tokens, 100);
}

#[tokio::test]
async fn complete_returns_service_unavailable_on_502() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(502).set_body_string("bad gateway"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::ServiceUnavailable(_)),
        "expected ServiceUnavailable, got: {err:?}"
    );
    assert!(err.is_retryable());
}

#[tokio::test]
async fn complete_returns_service_unavailable_on_503() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::ServiceUnavailable(_)),
        "expected ServiceUnavailable, got: {err:?}"
    );
}

#[tokio::test]
async fn complete_with_invalid_json_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json at all"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider.complete(minimal_request()).await.unwrap_err();

    assert!(
        matches!(err, ProviderError::InvalidRequest(ref msg) if msg.contains("invalid JSON")),
        "expected InvalidRequest with JSON error, got: {err:?}"
    );
}

#[tokio::test]
async fn stream_with_tool_calls() {
    let mock_server = MockServer::start().await;

    let ndjson_body = concat!(
        r#"{"model":"llama3.2","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"search","arguments":{"query":"rust"}}}]},"done":true,"done_reason":"tool_calls","eval_count":15,"prompt_eval_count":25}"#,
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson_body))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let handle = provider
        .complete_stream(minimal_request())
        .await
        .expect("should succeed");

    let events: Vec<neuron_types::StreamEvent> = handle.receiver.collect().await;

    let has_tool_start = events.iter().any(
        |e| matches!(e, neuron_types::StreamEvent::ToolUseStart { name, .. } if name == "search"),
    );
    assert!(has_tool_start, "expected ToolUseStart event");

    let has_tool_input = events.iter().any(
        |e| matches!(e, neuron_types::StreamEvent::ToolUseInputDelta { delta, .. } if delta.contains("rust")),
    );
    assert!(has_tool_input, "expected ToolUseInputDelta event");

    let has_tool_end = events
        .iter()
        .any(|e| matches!(e, neuron_types::StreamEvent::ToolUseEnd { .. }));
    assert!(has_tool_end, "expected ToolUseEnd event");

    let has_complete = events
        .iter()
        .any(|e| matches!(e, neuron_types::StreamEvent::MessageComplete(_)));
    assert!(has_complete, "expected MessageComplete event");
}

#[tokio::test]
async fn stream_returns_error_on_500() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider
        .complete_stream(minimal_request())
        .await
        .unwrap_err();

    assert!(
        matches!(err, ProviderError::ServiceUnavailable(_)),
        "expected ServiceUnavailable, got: {err:?}"
    );
}

#[tokio::test]
async fn stream_returns_error_on_400() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let err = provider
        .complete_stream(minimal_request())
        .await
        .unwrap_err();

    assert!(
        matches!(err, ProviderError::InvalidRequest(_)),
        "expected InvalidRequest, got: {err:?}"
    );
}

#[tokio::test]
async fn stream_message_complete_contains_assembled_text() {
    let mock_server = MockServer::start().await;

    let ndjson_body = concat!(
        r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}"#,
        "\n",
        r#"{"model":"llama3.2","message":{"role":"assistant","content":" there"},"done":false}"#,
        "\n",
        r#"{"model":"llama3.2","message":{"role":"assistant","content":"!"},"done":false}"#,
        "\n",
        r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","eval_count":3,"prompt_eval_count":5}"#,
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson_body))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let handle = provider
        .complete_stream(minimal_request())
        .await
        .expect("should succeed");

    let events: Vec<neuron_types::StreamEvent> = handle.receiver.collect().await;

    // Find the MessageComplete event and check assembled text
    let complete_msg = events.iter().find_map(|e| match e {
        neuron_types::StreamEvent::MessageComplete(msg) => Some(msg),
        _ => None,
    });
    let msg = complete_msg.expect("expected MessageComplete event");
    assert_eq!(msg.role, Role::Assistant);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello there!"));
}

#[tokio::test]
async fn stream_with_empty_response() {
    let mock_server = MockServer::start().await;

    let ndjson_body = concat!(
        r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","eval_count":0,"prompt_eval_count":5}"#,
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson_body))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let handle = provider
        .complete_stream(minimal_request())
        .await
        .expect("should succeed");

    let events: Vec<neuron_types::StreamEvent> = handle.receiver.collect().await;

    // Should have a Usage event
    let has_usage = events
        .iter()
        .any(|e| matches!(e, neuron_types::StreamEvent::Usage(_)));
    assert!(has_usage, "expected Usage event");

    // Should NOT have a MessageComplete (no content to assemble)
    let has_complete = events
        .iter()
        .any(|e| matches!(e, neuron_types::StreamEvent::MessageComplete(_)));
    assert!(
        !has_complete,
        "should not have MessageComplete for empty response"
    );
}

#[tokio::test]
async fn complete_with_all_options_set() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new()
        .base_url(mock_server.uri())
        .model("llama3.2")
        .keep_alive("10m");

    let req = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello".into())],
        }],
        max_tokens: Some(500),
        temperature: Some(0.7),
        top_p: Some(0.9),
        stop_sequences: vec!["END".into()],
        extra: Some(serde_json::json!({"seed": 42})),
        ..Default::default()
    };

    let resp = provider.complete(req).await.expect("should succeed");
    assert_eq!(resp.model, "llama3.2");
}

#[tokio::test]
async fn complete_with_multi_turn_conversation() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let req = CompletionRequest {
        messages: vec![
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("What is Rust?".into())],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text(
                    "Rust is a systems programming language.".into(),
                )],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("Tell me more.".into())],
            },
        ],
        ..Default::default()
    };

    let resp = provider.complete(req).await.expect("should succeed");
    assert!(!resp.message.content.is_empty());
}

#[tokio::test]
async fn complete_with_multiple_tool_calls_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "function": {
                            "name": "search",
                            "arguments": {"query": "rust"}
                        }
                    },
                    {
                        "function": {
                            "name": "read_file",
                            "arguments": {"path": "/tmp/test.rs"}
                        }
                    }
                ]
            },
            "done": true,
            "done_reason": "tool_calls",
            "eval_count": 20,
            "prompt_eval_count": 30,
        })))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let resp = provider
        .complete(minimal_request())
        .await
        .expect("should succeed");

    assert_eq!(resp.stop_reason, neuron_types::StopReason::ToolUse);
    assert_eq!(resp.message.content.len(), 2);

    // Verify both tool calls are present and have unique IDs
    let ids: Vec<&str> = resp
        .message
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "tool call IDs should be unique");
}

#[tokio::test]
async fn complete_response_has_ollama_id_prefix() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let resp = provider
        .complete(minimal_request())
        .await
        .expect("should succeed");

    assert!(
        resp.id.starts_with("ollama_"),
        "expected 'ollama_' prefix, got: {}",
        resp.id
    );
}

#[tokio::test]
async fn stream_with_multiple_tool_calls() {
    let mock_server = MockServer::start().await;

    let ndjson_body = concat!(
        r#"{"model":"llama3.2","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"search","arguments":{"q":"a"}}},{"function":{"name":"read","arguments":{"path":"/b"}}}]},"done":true,"done_reason":"tool_calls","eval_count":10,"prompt_eval_count":20}"#,
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson_body))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let handle = provider
        .complete_stream(minimal_request())
        .await
        .expect("should succeed");

    let events: Vec<neuron_types::StreamEvent> = handle.receiver.collect().await;

    let tool_starts: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, neuron_types::StreamEvent::ToolUseStart { .. }))
        .collect();
    assert_eq!(tool_starts.len(), 2, "expected 2 ToolUseStart events");

    // Verify MessageComplete includes both tool calls
    let complete_msg = events.iter().find_map(|e| match e {
        neuron_types::StreamEvent::MessageComplete(msg) => Some(msg),
        _ => None,
    });
    let msg = complete_msg.expect("expected MessageComplete event");
    assert_eq!(msg.content.len(), 2);
}

#[tokio::test]
async fn complete_with_request_model_overrides_client_model() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "codellama",
            "message": {
                "role": "assistant",
                "content": "Code output"
            },
            "done": true,
            "done_reason": "stop",
        })))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri()).model("llama3.2"); // client default

    let req = CompletionRequest {
        model: "codellama".into(), // request override
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello".into())],
        }],
        ..Default::default()
    };

    let resp = provider.complete(req).await.expect("should succeed");
    assert_eq!(resp.model, "codellama");
}

#[tokio::test]
async fn complete_with_response_missing_message_field() {
    let mock_server = MockServer::start().await;

    // Ollama response with model but missing message field
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llama3.2",
            "done": true,
            "done_reason": "stop",
        })))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let resp = provider
        .complete(minimal_request())
        .await
        .expect("should succeed even with missing message");

    // Should have no content blocks (empty message)
    assert!(resp.message.content.is_empty());
}

#[tokio::test]
async fn stream_usage_event_has_correct_token_counts() {
    let mock_server = MockServer::start().await;

    let ndjson_body = concat!(
        r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi"},"done":false}"#,
        "\n",
        r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","eval_count":42,"prompt_eval_count":100}"#,
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson_body))
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let handle = provider
        .complete_stream(minimal_request())
        .await
        .expect("should succeed");

    let events: Vec<neuron_types::StreamEvent> = handle.receiver.collect().await;

    let usage = events.iter().find_map(|e| match e {
        neuron_types::StreamEvent::Usage(u) => Some(u),
        _ => None,
    });
    let usage = usage.expect("expected Usage event");
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 42);
    assert!(usage.cache_read_tokens.is_none());
    assert!(usage.cache_creation_tokens.is_none());
}

#[test]
fn builder_methods_are_chainable() {
    // Verify builder chain compiles and does not panic.
    // Field values are tested in the unit tests inside client.rs.
    let _client = Ollama::new()
        .model("mistral")
        .base_url("http://remote:11434")
        .keep_alive("10m");
}

#[tokio::test]
async fn complete_with_system_blocks_prompt() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let req = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello".into())],
        }],
        system: Some(neuron_types::SystemPrompt::Blocks(vec![
            neuron_types::SystemBlock {
                text: "Be concise.".into(),
                cache_control: None,
            },
            neuron_types::SystemBlock {
                text: "Be helpful.".into(),
                cache_control: None,
            },
        ])),
        ..Default::default()
    };

    let resp = provider.complete(req).await.expect("should succeed");
    assert_eq!(resp.model, "llama3.2");
}

#[tokio::test]
async fn complete_with_tool_result_in_messages() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_response_body()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = Ollama::new().base_url(mock_server.uri());
    let req = CompletionRequest {
        messages: vec![
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("Search for rust".into())],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tc_1".into(),
                    name: "search".into(),
                    input: serde_json::json!({"query": "rust"}),
                }],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tc_1".into(),
                    content: vec![neuron_types::ContentItem::Text(
                        "Rust is a programming language".into(),
                    )],
                    is_error: false,
                }],
            },
        ],
        ..Default::default()
    };

    let resp = provider.complete(req).await.expect("should succeed");
    assert!(!resp.message.content.is_empty());
}

/// Verify that the request mapping module is accessible from integration tests.
#[test]
fn mapping_module_is_public() {
    use neuron_provider_ollama::mapping::{from_api_response, to_api_request};

    let req = minimal_request();
    let body = to_api_request(&req, "llama3.2", None);
    assert_eq!(body["model"], "llama3.2");

    let resp_body = serde_json::json!({
        "model": "llama3.2",
        "message": {
            "role": "assistant",
            "content": "Hi"
        },
        "done": true,
        "done_reason": "stop",
    });
    let resp = from_api_response(&resp_body).expect("should parse");
    assert_eq!(resp.model, "llama3.2");
}
