//! Integration tests for agent-loop.

use std::future::Future;
use std::sync::{Arc, Mutex};

use agent_context::SlidingWindowStrategy;
use agent_loop::{AgentLoop, LoopConfig};
use agent_tool::ToolRegistry;
use agent_types::{
    CompletionRequest, CompletionResponse, ContentBlock, Message, ProviderError, Role,
    StopReason, StreamHandle, SystemPrompt, TokenUsage,
};

/// A mock provider that returns pre-configured responses in sequence.
struct MockProvider {
    responses: Mutex<Vec<CompletionResponse>>,
}

impl MockProvider {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl agent_types::Provider for MockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        let response = {
            let mut responses = self.responses.lock().expect("test lock poisoned");
            if responses.is_empty() {
                panic!("MockProvider: no more responses configured");
            }
            responses.remove(0)
        };
        async move { Ok(response) }
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        async { Err(ProviderError::InvalidRequest("streaming not implemented in mock".into())) }
    }
}

/// Helper to create a simple text CompletionResponse.
fn text_response(text: &str) -> CompletionResponse {
    CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text(text.to_string())],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    }
}

/// Helper to create a tool_use CompletionResponse.
fn tool_use_response(tool_id: &str, tool_name: &str, input: serde_json::Value) -> CompletionResponse {
    CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: tool_id.to_string(),
                name: tool_name.to_string(),
                input,
            }],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason: StopReason::ToolUse,
    }
}

// --- Task 5.2 tests ---

#[test]
fn test_loop_config_defaults() {
    let config = LoopConfig::default();
    assert!(config.max_turns.is_none());
    assert!(!config.parallel_tool_execution);
    match &config.system_prompt {
        SystemPrompt::Text(text) => assert!(text.is_empty()),
        _ => panic!("expected Text variant for default system prompt"),
    }
}

#[test]
fn test_agent_loop_construction() {
    let provider = MockProvider::new(vec![]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        system_prompt: SystemPrompt::Text("You are a helpful assistant.".to_string()),
        max_turns: Some(5),
        parallel_tool_execution: false,
    };

    let agent = AgentLoop::new(provider, tools, context, config);

    assert_eq!(agent.config().max_turns, Some(5));
    assert!(!agent.config().parallel_tool_execution);
    assert!(agent.messages().is_empty());
}
