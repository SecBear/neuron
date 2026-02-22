//! Composition integration tests.
//!
//! These tests verify the composition examples from the design doc
//! compile and work correctly with mock providers.

use neuron::prelude::*;
use neuron::tool::ToolRegistry;
use neuron_types::ContextStrategy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// --- Mock Provider ---

struct MockProvider {
    responses: std::sync::Mutex<Vec<CompletionResponse>>,
}

impl MockProvider {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses),
        }
    }

    fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text(text.to_string())],
            },
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: None,
                cache_creation_tokens: None,
                reasoning_tokens: None,
                iterations: None,
            },
            model: "mock".to_string(),
            id: "mock-1".to_string(),
        }
    }

    fn tool_response(
        tool_name: &str,
        tool_id: &str,
        args: serde_json::Value,
    ) -> CompletionResponse {
        CompletionResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: tool_id.to_string(),
                    name: tool_name.to_string(),
                    input: args,
                }],
            },
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: None,
                cache_creation_tokens: None,
                reasoning_tokens: None,
                iterations: None,
            },
            model: "mock".to_string(),
            id: "mock-2".to_string(),
        }
    }
}

impl Provider for MockProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, neuron_types::ProviderError> {
        let response = self
            .responses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(0);
        Ok(response)
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<neuron_types::StreamHandle, neuron_types::ProviderError> {
        Err(neuron_types::ProviderError::Other(
            "streaming not supported in mock".into(),
        ))
    }
}

// --- Mock Tool ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct EchoArgs {
    text: String,
}

#[derive(Debug, Serialize)]
struct EchoOutput {
    echoed: String,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
enum EchoError {
    #[error("echo failed")]
    Failed,
}

struct EchoTool;

impl Tool for EchoTool {
    const NAME: &'static str = "echo";
    type Args = EchoArgs;
    type Output = EchoOutput;
    type Error = EchoError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            title: None,
            description: "Echo text back".into(),
            input_schema: serde_json::to_value(schemars::schema_for!(EchoArgs)).unwrap(),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(
        &self,
        args: Self::Args,
        _ctx: &ToolContext,
    ) -> Result<Self::Output, Self::Error> {
        Ok(EchoOutput {
            echoed: args.text,
        })
    }
}

fn user_msg(text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

fn test_ctx() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    }
}

// === Test 1: Minimal agent (3 blocks) ===

#[tokio::test]
async fn minimal_agent_text_response() {
    let provider = MockProvider::new(vec![MockProvider::text_response("Paris")]);
    let context = neuron::context::SlidingWindowStrategy::new(10, 100_000);
    let tools = ToolRegistry::new();

    let config = LoopConfig {
        system_prompt: neuron_types::SystemPrompt::Text("You are a helpful assistant.".into()),
        ..Default::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let ctx = test_ctx();

    let result = agent
        .run(user_msg("What is the capital of France?"), &ctx)
        .await
        .unwrap();
    assert_eq!(result.response, "Paris");
    assert_eq!(result.turns, 1);
}

// === Test 2: Agent with tool execution ===

#[tokio::test]
async fn agent_with_tool_calls() {
    let provider = MockProvider::new(vec![
        MockProvider::tool_response("echo", "call-1", serde_json::json!({"text": "hello"})),
        MockProvider::text_response("I echoed: hello"),
    ]);
    let context = neuron::context::SlidingWindowStrategy::new(10, 100_000);
    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let config = LoopConfig {
        system_prompt: neuron_types::SystemPrompt::Text("You can echo text.".into()),
        ..Default::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let ctx = test_ctx();

    let result = agent.run(user_msg("Echo hello"), &ctx).await.unwrap();
    assert_eq!(result.response, "I echoed: hello");
    assert_eq!(result.turns, 2);
}

// === Test 3: Agent with middleware ===

#[tokio::test]
async fn agent_with_middleware() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();

    let provider = MockProvider::new(vec![
        MockProvider::tool_response("echo", "call-1", serde_json::json!({"text": "world"})),
        MockProvider::text_response("done"),
    ]);
    let context = neuron::context::SlidingWindowStrategy::new(10, 100_000);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);
    tools.add_middleware(neuron::tool::tool_middleware_fn(
        move |call, ctx, next| {
            let c = counter.clone();
            Box::pin(async move {
                c.fetch_add(1, Ordering::SeqCst);
                next.run(call, ctx).await
            })
        },
    ));

    let config = LoopConfig {
        system_prompt: neuron_types::SystemPrompt::Text("Echo with logging.".into()),
        ..Default::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let ctx = test_ctx();

    let result = agent.run(user_msg("Echo world"), &ctx).await.unwrap();
    assert_eq!(result.response, "done");
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

// === Test 4: Max turns limit ===

#[tokio::test]
async fn agent_respects_max_turns() {
    let provider = MockProvider::new(vec![
        MockProvider::tool_response("echo", "call-1", serde_json::json!({"text": "1"})),
        MockProvider::tool_response("echo", "call-2", serde_json::json!({"text": "2"})),
        MockProvider::tool_response("echo", "call-3", serde_json::json!({"text": "3"})),
    ]);
    let context = neuron::context::SlidingWindowStrategy::new(10, 100_000);
    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let config = LoopConfig {
        system_prompt: neuron_types::SystemPrompt::Text("Keep echoing.".into()),
        max_turns: Some(2),
        ..Default::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let ctx = test_ctx();

    let result = agent.run(user_msg("Go"), &ctx).await;
    assert!(result.is_err());
}

// === Test 5: Feature-gated modules accessible ===

#[test]
fn prelude_types_accessible() {
    let _msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("hello".to_string())],
    };

    let _usage = TokenUsage {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: None,
        cache_creation_tokens: None,
        reasoning_tokens: None,
        iterations: None,
    };

    let _config = LoopConfig::default();
}

#[cfg(feature = "anthropic")]
#[test]
fn anthropic_module_accessible() {
    let _provider = neuron::anthropic::Anthropic::new("test-key");
}

// === Test 6: Context strategies compose ===

#[tokio::test]
async fn context_strategies_compose() {
    use neuron::context::*;

    let strategy = CompositeStrategy::new(
        vec![
            BoxedStrategy::new(SlidingWindowStrategy::new(10, 100_000)),
            BoxedStrategy::new(ToolResultClearingStrategy::new(10, 100_000)),
        ],
        100_000,
    );

    let msgs = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text("hello".to_string())],
    }];

    assert!(!strategy.should_compact(&msgs, 100_000));
}
