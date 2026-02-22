//! Multi-turn conversation: call run_text() multiple times showing
//! conversation history accumulation.
//!
//! Uses a MockProvider — no API key needed.
//!
//! Run with: cargo run --example multi_turn -p neuron-loop

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use neuron_context::SlidingWindowStrategy;
use neuron_loop::AgentLoop;
use neuron_tool::ToolRegistry;
use neuron_types::{
    CompletionRequest, CompletionResponse, ContentBlock, Message, ProviderError, Role, StopReason,
    StreamHandle, TokenUsage, ToolContext,
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
    // Three sequential text responses — no tool use.
    let provider = MockProvider::new(vec![
        text_response("The capital of France is Paris."),
        text_response("Paris has a population of about 2.1 million people in the city proper."),
        text_response("The Eiffel Tower, built in 1889, is the most iconic landmark in Paris."),
    ]);

    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(20, 100_000);

    let mut agent = AgentLoop::builder(provider, context)
        .tools(tools)
        .system_prompt("You are a helpful geography assistant.")
        .max_turns(10)
        .build();

    let ctx = tool_ctx();

    // --- Turn 1 ---
    println!("=== Turn 1 ===");
    let result = agent
        .run_text("What is the capital of France?", &ctx)
        .await
        .expect("turn 1 should complete");
    println!("Response: {}", result.response);
    println!("Messages in conversation: {}", agent.messages().len());
    println!();

    // --- Turn 2 ---
    println!("=== Turn 2 ===");
    let result = agent
        .run_text("How many people live there?", &ctx)
        .await
        .expect("turn 2 should complete");
    println!("Response: {}", result.response);
    println!("Messages in conversation: {}", agent.messages().len());
    println!();

    // --- Turn 3 ---
    println!("=== Turn 3 ===");
    let result = agent
        .run_text("What is its most famous landmark?", &ctx)
        .await
        .expect("turn 3 should complete");
    println!("Response: {}", result.response);
    println!("Messages in conversation: {}", agent.messages().len());
    println!();

    // Show total usage across all turns
    println!("=== Summary ===");
    println!("Total messages: {}", agent.messages().len());
    println!(
        "Final turn usage: {} input, {} output tokens",
        result.usage.input_tokens, result.usage.output_tokens
    );
}
