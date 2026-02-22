//! End-to-end agent using Anthropic, a tool registry, and the agent loop.
//!
//! Requires the ANTHROPIC_API_KEY environment variable to be set.
//!
//! Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --example full_agent -p neuron --features full
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use neuron::anthropic::Anthropic;
use neuron::context::SlidingWindowStrategy;
use neuron::prelude::*;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// A simple calculator tool for the agent to use
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CalculateArgs {
    /// A mathematical expression like "2 + 2"
    expression: String,
}

#[derive(Debug, serde::Serialize)]
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
            title: Some("Calculator".into()),
            description: "Evaluate a simple math expression. Supports +, -, *, /.".into(),
            input_schema: serde_json::to_value(schemars::schema_for!(CalculateArgs))
                .expect("schema serialization"),
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
        let expr = args.expression.trim();
        let result = eval_simple(expr).ok_or_else(|| CalculateError::Invalid(expr.to_string()))?;
        Ok(CalculateOutput { result })
    }
}

fn eval_simple(expr: &str) -> Option<f64> {
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
    expr.parse().ok()
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create an Anthropic provider from the environment variable.
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable must be set");
    let provider = Anthropic::new(api_key).model("claude-haiku-4-5-20251001");

    // 2. Create a ToolRegistry and register a tool.
    let mut tools = ToolRegistry::new();
    tools.register(CalculateTool);

    // 3. Create a SlidingWindowStrategy for context management.
    //    Keep at most 20 messages, targeting a 100k token window.
    let context = SlidingWindowStrategy::new(20, 100_000);

    // 4. Build an AgentLoop with the builder.
    let mut agent = AgentLoop::builder(provider, context)
        .tools(tools)
        .system_prompt(
            "You are a helpful math assistant. Use the calculate tool for arithmetic. \
             After getting the result, respond with a short sentence stating the answer.",
        )
        .max_turns(5)
        .build();

    // 5. Run the agent with a user prompt.
    let tool_ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "example-session".into(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
    };

    let result = agent
        .run_text("What is 42 * 17? Use the calculate tool.", &tool_ctx)
        .await?;

    // 6. Print the response.
    println!("Agent response: {}", result.response);
    println!("Turns taken:    {}", result.turns);
    println!(
        "Token usage:    {} input / {} output",
        result.usage.input_tokens, result.usage.output_tokens
    );

    Ok(())
}
