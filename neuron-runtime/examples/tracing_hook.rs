//! TracingHook: structured tracing output from the agentic loop.
//!
//! No API key needed — constructs events manually to demonstrate output.
//! Set RUST_LOG=debug to see all events.
//!
//! Run with: RUST_LOG=debug cargo run --example tracing_hook -p neuron-runtime

use neuron_runtime::TracingHook;
use neuron_types::{
    CompletionRequest, CompletionResponse, ContentBlock, ContentItem, HookEvent, Message,
    ObservabilityHook, StopReason, TokenUsage, ToolOutput,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber (respects RUST_LOG env var).
    tracing_subscriber::fmt::init();

    let hook = TracingHook::new();

    // Simulate a session starting.
    hook.on_event(HookEvent::SessionStart {
        session_id: "demo-session",
    })
    .await?;

    // Simulate the first loop iteration.
    hook.on_event(HookEvent::LoopIteration { turn: 1 }).await?;

    // Simulate an LLM call.
    let request = CompletionRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        messages: vec![Message::user("What is Rust?")],
        ..Default::default()
    };
    hook.on_event(HookEvent::PreLlmCall { request: &request })
        .await?;

    let response = CompletionResponse {
        id: "msg_001".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        message: Message::assistant("Rust is a systems programming language."),
        usage: TokenUsage {
            input_tokens: 12,
            output_tokens: 25,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    };
    hook.on_event(HookEvent::PostLlmCall {
        response: &response,
    })
    .await?;

    // Simulate a tool execution.
    let tool_input = serde_json::json!({"query": "Rust programming"});
    hook.on_event(HookEvent::PreToolExecution {
        tool_name: "web_search",
        input: &tool_input,
    })
    .await?;

    let output = ToolOutput {
        content: vec![ContentItem::Text("Found 10 results".to_string())],
        structured_content: None,
        is_error: false,
    };
    hook.on_event(HookEvent::PostToolExecution {
        tool_name: "web_search",
        output: &output,
    })
    .await?;

    // Simulate context compaction (e.g. triggered by the loop when tokens are high).
    hook.on_event(HookEvent::ContextCompaction {
        old_tokens: 50_000,
        new_tokens: 25_000,
    })
    .await?;

    // Simulate session end.
    hook.on_event(HookEvent::SessionEnd {
        session_id: "demo-session",
    })
    .await?;

    println!("\nAll 8 hook event types demonstrated.");
    println!("Run with RUST_LOG=debug to see the tracing output.");

    // Show the assistant's reply alongside message role.
    println!("\nAssistant reply:");
    for block in &response.message.content {
        if let ContentBlock::Text(t) = block {
            println!("  [{:?}] {t}", response.message.role);
        }
    }

    Ok(())
}
