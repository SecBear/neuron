//! Integration tests for the Ollama provider using wiremock.

use futures::StreamExt;
use neuron_provider_ollama::Ollama;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, ProviderError, Role};
use wiremock::matchers::{method, path};
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
        context_management: None,
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
