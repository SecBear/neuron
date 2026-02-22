//! Example: build and run an AgentLoop with a mock provider.
//!
//! Since we cannot call a real LLM provider without API keys, this example
//! uses a MockProvider that returns canned responses. It demonstrates the
//! builder pattern, tool registration, and loop execution.
//!
//! Run with: `cargo run --example agent_loop -p neuron-loop`

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use neuron_context::SlidingWindowStrategy;
use neuron_loop::AgentLoop;
use neuron_tool::ToolRegistry;
use neuron_types::{
    CompletionRequest, CompletionResponse, ContentBlock, ContentItem, Message, ProviderError,
    Role, StopReason, StreamHandle, TokenUsage, Tool, ToolContext, ToolDefinition,
};
use tokio_util::sync::CancellationToken;

// --- Mock provider ---

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

impl neuron_types::Provider for MockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        let response = {
            let mut responses = self.responses.lock().expect("lock poisoned");
            if responses.is_empty() {
                return std::future::ready(Err(ProviderError::Other(
                    "no more mock responses".into(),
                )));
            }
            responses.remove(0)
        };
        std::future::ready(Ok(response))
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        std::future::ready(Err(ProviderError::InvalidRequest(
            "streaming not supported in mock".into(),
        )))
    }
}

// --- Mock tool ---

struct GreetTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GreetArgs {
    name: String,
}

impl Tool for GreetTool {
    const NAME: &'static str = "greet";
    type Args = GreetArgs;
    type Output = String;
    type Error = std::io::Error;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "greet".to_string(),
            title: Some("Greet".to_string()),
            description: "Greet someone by name".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(&self, args: GreetArgs, _ctx: &ToolContext) -> Result<String, std::io::Error> {
        Ok(format!("Hello, {}! Welcome to neuron.", args.name))
    }
}

// --- Helpers ---

fn text_response(text: &str) -> CompletionResponse {
    CompletionResponse {
        id: "mock-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text(text.to_string())],
        },
        usage: TokenUsage {
            input_tokens: 50,
            output_tokens: 20,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    }
}

fn tool_use_response(id: &str, name: &str, input: serde_json::Value) -> CompletionResponse {
    CompletionResponse {
        id: "mock-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input,
            }],
        },
        usage: TokenUsage {
            input_tokens: 50,
            output_tokens: 15,
            ..Default::default()
        },
        stop_reason: StopReason::ToolUse,
    }
}

fn tool_ctx() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "example-session".to_string(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
    }
}

#[tokio::main]
async fn main() {
    // The mock provider will:
    // 1. First call the greet tool
    // 2. Then return a final text response
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "greet", serde_json::json!({"name": "World"})),
        text_response("The greeting has been delivered successfully!"),
    ]);

    // Set up tools
    let mut tools = ToolRegistry::new();
    tools.register(GreetTool);

    // Context strategy: sliding window with generous limits
    let context = SlidingWindowStrategy::new(20, 100_000);

    // Build the agent loop using the builder pattern
    let mut agent = AgentLoop::builder(provider, context)
        .tools(tools)
        .system_prompt("You are a friendly assistant that greets people.")
        .max_turns(10)
        .build();

    println!("Agent loop configured:");
    println!("  Max turns: {:?}", agent.config().max_turns);
    println!("  Tool definitions: {:?}", agent.messages().len());
    println!();

    // Run the loop with a user message
    let ctx = tool_ctx();
    let result = agent
        .run_text("Please greet the World!", &ctx)
        .await
        .expect("agent loop should complete");

    // Print results
    println!("Agent response: {}", result.response);
    println!("Turns taken: {}", result.turns);
    println!(
        "Token usage: {} input, {} output",
        result.usage.input_tokens, result.usage.output_tokens
    );
    println!("Total messages in conversation: {}", result.messages.len());

    // Show conversation flow
    println!("\nConversation:");
    for msg in &result.messages {
        let role = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => "System",
        };
        for block in &msg.content {
            match block {
                ContentBlock::Text(t) => println!("  [{role}] {t}"),
                ContentBlock::ToolUse { name, input, .. } => {
                    println!("  [{role}] -> tool_use: {name}({input})")
                }
                ContentBlock::ToolResult { content, .. } => {
                    for item in content {
                        if let ContentItem::Text(t) = item {
                            println!("  [{role}] <- tool_result: {t}");
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
