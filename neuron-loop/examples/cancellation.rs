//! Example: cancellation via CancellationToken.
//!
//! Demonstrates cooperative cancellation of the agent loop using a
//! CancellationToken with a timeout. No API key needed — uses a mock provider.
//!
//! Run with: `cargo run --example cancellation -p neuron-loop`

use neuron_context::SlidingWindowStrategy;
use neuron_loop::AgentLoop;
use neuron_tool::ToolRegistry;
use neuron_types::*;

// --- Mock provider that always requests a tool call (infinite loop without cancellation) ---

struct SlowProvider;

impl Provider for SlowProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // Always return a tool call so the loop continues
        Ok(CompletionResponse {
            id: "resp-1".to_string(),
            model: "mock".to_string(),
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "call-1".to_string(),
                    name: "wait".to_string(),
                    input: serde_json::json!({}),
                }],
            },
            usage: TokenUsage::default(),
            stop_reason: StopReason::ToolUse,
        })
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest("not supported".into()))
    }
}

// --- Simple tool that sleeps ---

struct WaitTool;

impl Tool for WaitTool {
    const NAME: &'static str = "wait";
    type Args = serde_json::Value;
    type Output = String;
    type Error = ToolError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "wait".to_string(),
            title: None,
            description: "Wait briefly".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(&self, _args: Self::Args, _ctx: &ToolContext) -> Result<String, ToolError> {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        Ok("waited".to_string())
    }
}

#[tokio::main]
async fn main() {
    let context = SlidingWindowStrategy::new(100, 100_000);

    let mut tools = ToolRegistry::new();
    tools.register(WaitTool);

    let mut agent = AgentLoop::builder(SlowProvider, context)
        .tools(tools)
        .max_turns(100) // High limit — cancellation should stop it first
        .build();

    // Create a ToolContext with a cancellation token
    let ctx = ToolContext::default();
    let token = ctx.cancellation_token.clone();

    // Cancel after 300ms from a background task
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        println!("[canceller] Cancelling the loop...");
        token.cancel();
    });

    println!("[main] Starting agent loop (will be cancelled after ~300ms)...");

    match agent.run(Message::user("Do something"), &ctx).await {
        Err(LoopError::Cancelled) => {
            println!("[main] Loop was cancelled as expected!");
        }
        Ok(result) => {
            println!(
                "[main] Loop completed normally after {} turns",
                result.turns
            );
        }
        Err(e) => {
            println!("[main] Loop errored: {e}");
        }
    }
}
