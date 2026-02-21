//! Smoke tests against the real Anthropic API.
//!
//! These tests are `#[ignore]` by default. Run them with:
//!
//!     ANTHROPIC_API_KEY=sk-... cargo test --test smoke_anthropic -- --ignored
//!
//! They make real API calls, cost real money (fractions of a cent each),
//! and require network access. They validate that our request/response
//! mapping, streaming parser, and agent loop work end-to-end against
//! the actual Anthropic Messages API.

use agent_blocks::prelude::*;
use agent_blocks::tool::ToolRegistry;
use agent_types::{StreamEvent, ToolChoice};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn api_key() -> String {
    std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set")
}

fn anthropic() -> agent_blocks::anthropic::Anthropic {
    agent_blocks::anthropic::Anthropic::new(api_key())
        .model("claude-haiku-4-5-20251001") // cheapest model for smoke tests
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
        session_id: "smoke-test".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    }
}

// ---------------------------------------------------------------------------
// A simple tool for Claude to call
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CalculateArgs {
    /// A mathematical expression like "2 + 2"
    expression: String,
}

#[derive(Debug, Serialize)]
struct CalculateOutput {
    result: f64,
}

#[derive(Debug, thiserror::Error)]
enum CalculateError {
    #[error("cannot evaluate: {0}")]
    Invalid(String),
}

struct CalculateTool;

impl Tool for CalculateTool {
    const NAME: &'static str = "calculate";
    type Args = CalculateArgs;
    type Output = CalculateOutput;
    type Error = CalculateError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            title: None,
            description: "Evaluate a simple math expression. Supports +, -, *, /.".into(),
            input_schema: serde_json::to_value(schemars::schema_for!(CalculateArgs)).unwrap(),
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
        // Trivial evaluator — only handles "a op b" patterns
        let expr = args.expression.trim();
        let result = eval_simple(expr)
            .ok_or_else(|| CalculateError::Invalid(expr.to_string()))?;
        Ok(CalculateOutput { result })
    }
}

fn eval_simple(expr: &str) -> Option<f64> {
    // Try "a + b", "a - b", "a * b", "a / b"
    for op in [" + ", " - ", " * ", " / "] {
        if let Some((left, right)) = expr.split_once(op) {
            let a: f64 = left.trim().parse().ok()?;
            let b: f64 = right.trim().parse().ok()?;
            return Some(match op.trim() {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" => a / b,
                _ => return None,
            });
        }
    }
    // Maybe it's just a number
    expr.parse().ok()
}

// ===========================================================================
// Test 1: Basic completion — does our request format work?
// ===========================================================================

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn smoke_basic_completion() {
    let provider = anthropic();

    let request = CompletionRequest {
        model: "claude-haiku-4-5-20251001".into(),
        messages: vec![user_msg("What is 2+2? Reply with just the number.")],
        system: Some(SystemPrompt::Text(
            "You are a helpful assistant. Reply concisely.".into(),
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

    // Verify response structure
    assert!(!response.id.is_empty(), "response should have an ID");
    assert!(!response.model.is_empty(), "response should name the model");
    assert_eq!(response.message.role, Role::Assistant);
    assert!(!response.message.content.is_empty(), "should have content");
    assert!(response.usage.input_tokens > 0, "should report input tokens");
    assert!(
        response.usage.output_tokens > 0,
        "should report output tokens"
    );

    // The answer should contain "4" somewhere
    let text = match &response.message.content[0] {
        ContentBlock::Text(t) => t.clone(),
        other => panic!("expected Text, got {:?}", other),
    };
    assert!(text.contains("4"), "expected '4' in response, got: {text}");

    println!("  response: {text}");
    println!(
        "  tokens: {} in / {} out",
        response.usage.input_tokens, response.usage.output_tokens
    );
}

// ===========================================================================
// Test 2: Streaming — does our SSE parser work?
// ===========================================================================

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn smoke_streaming() {
    let provider = anthropic();

    let request = CompletionRequest {
        model: "claude-haiku-4-5-20251001".into(),
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
    let mut got_usage = false;
    let mut got_complete = false;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(delta) => {
                text_deltas.push(delta);
            }
            StreamEvent::Usage(usage) => {
                assert!(usage.input_tokens > 0);
                got_usage = true;
            }
            StreamEvent::MessageComplete(msg) => {
                assert_eq!(msg.role, Role::Assistant);
                got_complete = true;
            }
            StreamEvent::Error(e) => {
                panic!("stream error: {}", e.message);
            }
            _ => {} // ThinkingDelta, SignatureDelta, etc.
        }
    }

    assert!(!text_deltas.is_empty(), "should receive text deltas");
    assert!(got_usage, "should receive usage event");
    assert!(got_complete, "should receive message complete event");

    let full_text: String = text_deltas.into_iter().collect();
    println!("  streamed: {full_text}");
    assert!(
        full_text.contains("1") && full_text.contains("5"),
        "expected 1-5 in: {full_text}"
    );
}

// ===========================================================================
// Test 3: Tool use — does Claude correctly call our tool?
// ===========================================================================

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn smoke_tool_use() {
    let provider = anthropic();
    let calc = CalculateTool;
    let tool_def = Tool::definition(&calc);
    let tool_dyn: &dyn ToolDyn = &calc;

    // Step 1: Ask Claude to use the calculator
    let request = CompletionRequest {
        model: "claude-haiku-4-5-20251001".into(),
        messages: vec![user_msg("What is 137 * 42? Use the calculate tool.")],
        system: Some(SystemPrompt::Text(
            "You have a calculator tool. Always use it for math.".into(),
        )),
        tools: vec![tool_def],
        max_tokens: Some(256),
        temperature: Some(0.0),
        top_p: None,
        stop_sequences: vec![],
        tool_choice: Some(ToolChoice::Auto),
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };

    let response = provider.complete(request).await.unwrap();

    // Claude should respond with a tool use
    assert_eq!(
        response.stop_reason,
        StopReason::ToolUse,
        "expected tool_use stop reason, got {:?}",
        response.stop_reason
    );

    let tool_call = response
        .message
        .content
        .iter()
        .find_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => Some((id, name, input)),
            _ => None,
        })
        .expect("expected a ToolUse content block");

    assert_eq!(tool_call.1, "calculate");
    println!("  Claude called: {}({:?})", tool_call.1, tool_call.0);

    // Step 2: Execute the tool
    let ctx = tool_ctx();
    let tool_result = tool_dyn
        .call_dyn(tool_call.2.clone(), &ctx)
        .await
        .unwrap();
    assert!(!tool_result.is_error);
    println!(
        "  tool result: {:?}",
        tool_result.structured_content.as_ref().unwrap()
    );

    // Step 3: Send the tool result back, get final answer
    let messages = vec![
        user_msg("What is 137 * 42? Use the calculate tool."),
        response.message.clone(),
        Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: tool_call.0.clone(),
                content: tool_result
                    .content
                    .into_iter()
                    .map(|c| match c {
                        ContentItem::Text(t) => ContentItem::Text(t),
                        other => other,
                    })
                    .collect(),
                is_error: false,
            }],
        },
    ];

    let followup = CompletionRequest {
        model: "claude-haiku-4-5-20251001".into(),
        messages,
        system: Some(SystemPrompt::Text(
            "You have a calculator tool. Always use it for math.".into(),
        )),
        tools: vec![Tool::definition(&calc)],
        max_tokens: Some(256),
        temperature: Some(0.0),
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };

    let final_response = provider.complete(followup).await.unwrap();
    let final_text = final_response
        .message
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.clone()),
            _ => None,
        })
        .expect("expected text in final response");

    println!("  final: {final_text}");
    assert!(
        final_text.contains("5754") || final_text.contains("5,754"),
        "expected 5754 in: {final_text}"
    );
}

// ===========================================================================
// Test 4: Full agent loop — everything composed together
// ===========================================================================

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn smoke_full_agent_loop() {
    let provider = anthropic();

    let mut tools = ToolRegistry::new();
    tools.register(CalculateTool);

    let context = agent_blocks::context::SlidingWindowStrategy::new(10, 100_000);

    let config = LoopConfig {
        system_prompt: SystemPrompt::Text(
            "You are a math assistant. Use the calculate tool for any arithmetic. \
             After getting the result, respond with a short sentence stating the answer."
                .into(),
        ),
        max_turns: Some(5),
        parallel_tool_execution: false,
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let ctx = tool_ctx();

    let result = agent
        .run(
            user_msg("What is 99 * 101? Use the calculate tool."),
            &ctx,
        )
        .await
        .unwrap();

    println!("  response: {}", result.response);
    println!("  turns: {}", result.turns);
    println!(
        "  tokens: {} in / {} out",
        result.usage.input_tokens, result.usage.output_tokens
    );

    // Should have taken 2 turns: tool call + final response
    assert!(
        result.turns >= 2,
        "expected at least 2 turns (tool call + response), got {}",
        result.turns
    );

    // Answer should contain 9999
    assert!(
        result.response.contains("9999") || result.response.contains("9,999"),
        "expected 9999 in: {}",
        result.response
    );
}
