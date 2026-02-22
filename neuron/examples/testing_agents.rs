//! Example: testing agents with mock providers and tools.
//!
//! Demonstrates patterns for unit testing agents without real API calls.
//! No API key needed — everything is mocked.
//!
//! Run with: `cargo run --example testing_agents -p neuron`

use std::sync::Mutex;

use neuron_loop::AgentLoop;
use neuron_tool::ToolRegistry;
use neuron_types::*;

// --- Mock provider: returns pre-configured responses ---

struct MockProvider {
    responses: Mutex<Vec<CompletionResponse>>,
}

impl MockProvider {
    fn with_responses(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }

    /// Helper: create a simple text response.
    fn text(text: &str) -> CompletionResponse {
        CompletionResponse {
            id: "mock-id".to_string(),
            model: "mock-model".to_string(),
            message: Message::assistant(text),
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
        }
    }

    /// Helper: create a response that calls a tool.
    fn tool_call(tool_name: &str, input: serde_json::Value) -> CompletionResponse {
        CompletionResponse {
            id: "mock-id".to_string(),
            model: "mock-model".to_string(),
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "call-1".to_string(),
                    name: tool_name.to_string(),
                    input,
                }],
            },
            usage: TokenUsage::default(),
            stop_reason: StopReason::ToolUse,
        }
    }
}

impl Provider for MockProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            Ok(MockProvider::text("No more responses configured"))
        } else {
            Ok(responses.remove(0))
        }
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest(
            "streaming not supported in mock".to_string(),
        ))
    }
}

// --- No-op context strategy (never compacts) ---

struct NoCompaction;

impl ContextStrategy for NoCompaction {
    fn should_compact(&self, _messages: &[Message], _token_count: usize) -> bool {
        false
    }

    async fn compact(&self, _messages: Vec<Message>) -> Result<Vec<Message>, ContextError> {
        unreachable!()
    }

    fn token_estimate(&self, _messages: &[Message]) -> usize {
        0
    }
}

// --- A simple tool for testing ---

struct AddTool;

impl Tool for AddTool {
    const NAME: &'static str = "add";
    type Args = serde_json::Value;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "add".to_string(),
            title: None,
            description: "Add two numbers".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(
        &self,
        args: Self::Args,
        _ctx: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let a = args["a"].as_f64().unwrap_or(0.0);
        let b = args["b"].as_f64().unwrap_or(0.0);
        Ok(serde_json::json!({"result": a + b}))
    }
}

#[tokio::main]
async fn main() {
    println!("=== Test 1: Simple single-turn response ===");
    {
        let provider = MockProvider::with_responses(vec![MockProvider::text(
            "The answer is 42.",
        )]);
        let mut agent = AgentLoop::builder(provider, NoCompaction)
            .tools(ToolRegistry::new())
            .max_turns(1)
            .build();
        let ctx = ToolContext::default();
        let result = agent.run(Message::user("What is the answer?"), &ctx).await.unwrap();
        println!("  Response: {}", result.response);
        println!("  Turns: {}", result.turns);
        assert_eq!(result.turns, 1);
        assert!(result.response.contains("42"));
    }

    println!("\n=== Test 2: Tool call then final response ===");
    {
        let provider = MockProvider::with_responses(vec![
            // First: model calls the add tool
            MockProvider::tool_call("add", serde_json::json!({"a": 3, "b": 4})),
            // Second: model returns final answer after seeing tool result
            MockProvider::text("3 + 4 = 7"),
        ]);

        let mut tools = ToolRegistry::new();
        tools.register(AddTool);

        let mut agent = AgentLoop::builder(provider, NoCompaction)
            .tools(tools)
            .max_turns(5)
            .build();
        let ctx = ToolContext::default();
        let result = agent
            .run(Message::user("What is 3 + 4?"), &ctx)
            .await
            .unwrap();
        println!("  Response: {}", result.response);
        println!("  Turns: {}", result.turns);
        assert_eq!(result.turns, 2);
        assert!(result.response.contains("7"));
    }

    println!("\n=== Test 3: Max turns limit ===");
    {
        // Provider always calls a tool — should hit max_turns
        let provider = MockProvider::with_responses(vec![
            MockProvider::tool_call("add", serde_json::json!({"a": 1, "b": 1})),
            MockProvider::tool_call("add", serde_json::json!({"a": 2, "b": 2})),
            MockProvider::tool_call("add", serde_json::json!({"a": 3, "b": 3})),
        ]);

        let mut tools = ToolRegistry::new();
        tools.register(AddTool);

        let mut agent = AgentLoop::builder(provider, NoCompaction)
            .tools(tools)
            .max_turns(2)
            .build();
        let ctx = ToolContext::default();
        let result = agent.run(Message::user("Keep adding"), &ctx).await;
        match result {
            Err(LoopError::MaxTurns(n)) => println!("  Hit max turns limit: {n}"),
            other => println!("  Unexpected: {other:?}"),
        }
    }

    println!("\nAll test patterns demonstrated.");
    println!("Use these patterns in your #[cfg(test)] modules with assert! macros.");
}
