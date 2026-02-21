//! Integration tests for agent-loop.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_context::SlidingWindowStrategy;
use agent_loop::{AgentLoop, LoopConfig};
use agent_tool::ToolRegistry;
use agent_types::{
    CompletionRequest, CompletionResponse, ContentBlock, ContentItem, LoopError, Message,
    ProviderError, Role, StopReason, StreamHandle, SystemPrompt, TokenUsage, Tool, ToolContext,
    ToolDefinition, ToolOutput,
};
use tokio_util::sync::CancellationToken;

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

/// A mock tool that echoes its input.
struct EchoTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EchoArgs {
    text: String,
}

impl Tool for EchoTool {
    const NAME: &'static str = "echo";
    type Args = EchoArgs;
    type Output = String;
    type Error = std::io::Error;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            title: Some("Echo".to_string()),
            description: "Echoes input text".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    fn call(
        &self,
        args: EchoArgs,
        _ctx: &ToolContext,
    ) -> impl Future<Output = Result<String, std::io::Error>> + Send {
        async move { Ok(format!("echo: {}", args.text)) }
    }
}

/// Helper to create a default ToolContext for tests.
fn test_tool_context() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test-session".to_string(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
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
fn tool_use_response(
    tool_id: &str,
    tool_name: &str,
    input: serde_json::Value,
) -> CompletionResponse {
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

// --- Task 5.3 tests ---

#[tokio::test]
async fn test_run_text_only_completes_in_one_turn() {
    let provider = MockProvider::new(vec![text_response("Hello, world!")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        system_prompt: SystemPrompt::Text("You are helpful.".to_string()),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let result = agent.run(user_msg, &test_tool_context()).await.expect("run should succeed");

    assert_eq!(result.response, "Hello, world!");
    assert_eq!(result.turns, 1);
    assert_eq!(result.usage.input_tokens, 10);
    assert_eq!(result.usage.output_tokens, 5);
}

#[tokio::test]
async fn test_run_with_tool_call_completes_in_two_turns() {
    // First response: model calls echo tool
    // Second response: model returns final text
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("The echo says: echo: hello"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo hello".to_string())],
    };

    let result = agent.run(user_msg, &test_tool_context()).await.expect("run should succeed");

    assert_eq!(result.response, "The echo says: echo: hello");
    assert_eq!(result.turns, 2);
    // Cumulative usage: 10+10 input, 5+5 output
    assert_eq!(result.usage.input_tokens, 20);
    assert_eq!(result.usage.output_tokens, 10);
}

#[tokio::test]
async fn test_run_max_turns_limit() {
    // Provider always returns tool calls — should be stopped by max_turns
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "1"})),
        tool_use_response("call-2", "echo", serde_json::json!({"text": "2"})),
        tool_use_response("call-3", "echo", serde_json::json!({"text": "3"})),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        max_turns: Some(2),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Keep echoing".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::MaxTurns(n) => assert_eq!(n, 2),
        other => panic!("expected MaxTurns error, got: {other:?}"),
    }
}
