use agent_types::*;
use std::future::Future;

/// A mock provider that always returns a fixed response.
struct MockProvider;

impl Provider for MockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        async {
            Ok(CompletionResponse {
                id: "mock_1".into(),
                model: "mock".into(),
                message: Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text("mock response".into())],
                },
                usage: TokenUsage::default(),
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        async { Err(ProviderError::Other("streaming not implemented".into())) }
    }
}

#[tokio::test]
async fn mock_provider_complete() {
    let provider = MockProvider;
    let request = CompletionRequest {
        model: "mock".into(),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("hi".into())],
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
    };
    let response = provider.complete(request).await.unwrap();
    assert_eq!(response.id, "mock_1");
    assert_eq!(response.stop_reason, StopReason::EndTurn);
}

#[tokio::test]
async fn mock_provider_stream_returns_error() {
    let provider = MockProvider;
    let request = CompletionRequest {
        model: "mock".into(),
        messages: vec![],
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
    };
    let err = provider.complete_stream(request).await.unwrap_err();
    assert!(err.to_string().contains("streaming not implemented"));
}
