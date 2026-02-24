//! Basic usage of neuron-otel OtelHook.
//!
//! Demonstrates creating an `OtelHook`, configuring it with `OtelConfig`,
//! and calling `on_event()` with each `HookEvent` variant. No API key or
//! external service needed — events are emitted as `tracing` spans.
//!
//! Set `RUST_LOG=debug` to see all span output.
//!
//! Run with: `RUST_LOG=debug cargo run --example basic -p neuron-otel`

use neuron_otel::{OtelConfig, OtelHook};
use neuron_types::{
    CompletionRequest, CompletionResponse, ContentItem, HookEvent, Message, ObservabilityHook,
    StopReason, TokenUsage, ToolOutput,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize a tracing subscriber so span output is visible.
    tracing_subscriber::fmt::init();

    // --- Default config (no content capture) ---
    let default_hook = OtelHook::default();
    println!("Created OtelHook with default config (content capture disabled).");

    // --- Custom config with content capture enabled ---
    let config = OtelConfig {
        capture_input: true,
        capture_output: true,
    };
    let hook = OtelHook::new(config);
    println!("Created OtelHook with content capture enabled.\n");

    // In a real application you would pass the hook to AgentLoop:
    //
    //   let loop_handle = AgentLoop::builder(provider, tool_registry)
    //       .hook(hook)
    //       .build();
    //
    // Below we call on_event() directly to demonstrate each event type.

    // 1. Session start
    let action = hook
        .on_event(HookEvent::SessionStart {
            session_id: "demo-session-001",
        })
        .await?;
    println!("SessionStart   -> {action:?}");

    // 2. Loop iteration
    let action = hook.on_event(HookEvent::LoopIteration { turn: 0 }).await?;
    println!("LoopIteration  -> {action:?}");

    // 3. Pre-LLM call
    let request = CompletionRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        messages: vec![Message::user("Explain the borrow checker in one sentence.")],
        ..Default::default()
    };
    let action = hook
        .on_event(HookEvent::PreLlmCall { request: &request })
        .await?;
    println!("PreLlmCall     -> {action:?}");

    // 4. Post-LLM call
    let response = CompletionResponse {
        id: "msg_demo_001".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        message: Message::assistant(
            "The borrow checker enforces that references never outlive the data they point to.",
        ),
        usage: TokenUsage {
            input_tokens: 18,
            output_tokens: 22,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    };
    let action = hook
        .on_event(HookEvent::PostLlmCall {
            response: &response,
        })
        .await?;
    println!("PostLlmCall    -> {action:?}");

    // 5. Pre-tool execution
    let tool_input = serde_json::json!({"query": "Rust borrow checker"});
    let action = hook
        .on_event(HookEvent::PreToolExecution {
            tool_name: "web_search",
            input: &tool_input,
        })
        .await?;
    println!("PreToolExec    -> {action:?}");

    // 6. Post-tool execution
    let tool_output = ToolOutput {
        content: vec![ContentItem::Text(
            "Found relevant documentation on ownership and borrowing.".to_string(),
        )],
        structured_content: None,
        is_error: false,
    };
    let action = hook
        .on_event(HookEvent::PostToolExecution {
            tool_name: "web_search",
            output: &tool_output,
        })
        .await?;
    println!("PostToolExec   -> {action:?}");

    // 7. Context compaction
    let action = hook
        .on_event(HookEvent::ContextCompaction {
            old_tokens: 100_000,
            new_tokens: 40_000,
        })
        .await?;
    println!("Compaction     -> {action:?}");

    // 8. Session end
    let action = hook
        .on_event(HookEvent::SessionEnd {
            session_id: "demo-session-001",
        })
        .await?;
    println!("SessionEnd     -> {action:?}");

    println!("\nAll HookEvent variants demonstrated.");
    println!("Every call returned HookAction::Continue (OtelHook is observe-only).");

    // Also demonstrate the default hook works identically.
    let action = default_hook
        .on_event(HookEvent::LoopIteration { turn: 42 })
        .await?;
    println!("\nDefault hook also returns: {action:?}");

    Ok(())
}
