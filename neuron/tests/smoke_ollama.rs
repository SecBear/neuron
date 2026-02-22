#![cfg(feature = "ollama")]
//! Smoke tests against a local Ollama instance.
//!
//! These tests are `#[ignore]` by default. Run them with:
//!
//!     cargo test -p neuron --features ollama --test smoke_ollama -- --ignored
//!
//! They require a running Ollama server with the `llama3.2` model pulled.
//! Start Ollama first: `ollama serve` then `ollama pull llama3.2`.
//!
//! Tool calling is not tested here because small local models are unreliable
//! with tool use. These tests validate basic completion and streaming.

use neuron::prelude::*;
use neuron::tool::ToolRegistry;
use neuron_types::StreamEvent;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ollama() -> neuron::ollama::Ollama {
    neuron::ollama::Ollama::new()
        .model("llama3.2")
        .keep_alive("0") // unload after test
}

fn user_msg(text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

fn tool_ctx() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "smoke-test-ollama".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    }
}

// ===========================================================================
// Test 1: Basic completion
// ===========================================================================

#[tokio::test]
#[ignore = "requires local Ollama"]
async fn smoke_basic_completion() {
    let provider = ollama();

    let request = CompletionRequest {
        model: "llama3.2".into(),
        messages: vec![user_msg("What is 2+2? Reply with just the number.")],
        system: Some(SystemPrompt::Text(
            "You are a helpful assistant. Reply concisely with only the answer.".into(),
        )),
        tools: vec![],
        max_tokens: Some(64),
        temperature: Some(0.0),
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };

    let response = provider.complete(request).await.unwrap();

    assert_eq!(response.message.role, Role::Assistant);
    assert!(!response.message.content.is_empty(), "should have content");

    let text = match &response.message.content[0] {
        ContentBlock::Text(t) => t.clone(),
        other => panic!("expected Text, got {other:?}"),
    };
    assert!(text.contains("4"), "expected '4' in response, got: {text}");

    println!("  response: {text}");
    println!(
        "  tokens: {} in / {} out",
        response.usage.input_tokens, response.usage.output_tokens
    );
}

// ===========================================================================
// Test 2: Streaming
// ===========================================================================

#[tokio::test]
#[ignore = "requires local Ollama"]
async fn smoke_streaming() {
    let provider = ollama();

    let request = CompletionRequest {
        model: "llama3.2".into(),
        messages: vec![user_msg(
            "Count from 1 to 5, separated by commas. Nothing else.",
        )],
        system: None,
        tools: vec![],
        max_tokens: Some(64),
        temperature: Some(0.0),
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };

    let stream_handle = provider.complete_stream(request).await.unwrap();
    let mut stream = stream_handle.receiver;

    let mut text_deltas = Vec::new();
    let mut got_complete = false;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(delta) => {
                text_deltas.push(delta);
            }
            StreamEvent::MessageComplete(msg) => {
                assert_eq!(msg.role, Role::Assistant);
                got_complete = true;
            }
            StreamEvent::Error(e) => {
                panic!("stream error: {}", e.message);
            }
            _ => {}
        }
    }

    assert!(!text_deltas.is_empty(), "should receive text deltas");
    assert!(got_complete, "should receive message complete event");

    let full_text: String = text_deltas.into_iter().collect();
    println!("  streamed: {full_text}");
    assert!(
        full_text.contains("1") && full_text.contains("5"),
        "expected 1-5 in: {full_text}"
    );
}

// ===========================================================================
// Test 3: Full agent loop (no tools — local models unreliable with tool calling)
// ===========================================================================

#[tokio::test]
#[ignore = "requires local Ollama"]
async fn smoke_full_neuron_loop() {
    let provider = ollama();

    let tools = ToolRegistry::new();
    let context = neuron::context::SlidingWindowStrategy::new(10, 100_000);

    let config = LoopConfig {
        system_prompt: SystemPrompt::Text(
            "You are a helpful assistant. Answer concisely in one sentence.".into(),
        ),
        max_turns: Some(1),
        parallel_tool_execution: false,
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let ctx = tool_ctx();

    let result = agent
        .run(user_msg("What is the capital of France?"), &ctx)
        .await
        .unwrap();

    println!("  response: {}", result.response);
    println!("  turns: {}", result.turns);

    assert_eq!(result.turns, 1, "should complete in 1 turn (no tools)");
    assert!(
        result.response.to_lowercase().contains("paris"),
        "expected 'paris' in: {}",
        result.response
    );
}
