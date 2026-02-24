//! Example: define a tool with `#[neuron_tool]`, register it, and execute it.
//!
//! Run with: `cargo run --example custom_tool -p neuron-tool`

use std::collections::HashMap;
use std::path::PathBuf;

use neuron_tool::{ToolRegistry, neuron_tool};
use neuron_types::{Tool, ToolContext};
use tokio_util::sync::CancellationToken;

// --- Output and error types for the calculator tool ---

#[derive(Debug, serde::Serialize)]
struct CalcOutput {
    result: f64,
}

#[derive(Debug, thiserror::Error)]
enum CalcError {
    #[error("unsupported operator: {0}")]
    UnsupportedOperator(String),
}

// --- Define the tool using the #[neuron_tool] macro ---

#[neuron_tool(
    name = "calculate",
    description = "Perform basic arithmetic on two numbers"
)]
async fn calculate(
    /// The left-hand operand
    left: f64,
    /// The right-hand operand
    right: f64,
    /// The operator: add, sub, mul, or div
    operator: String,
    _ctx: &ToolContext,
) -> Result<CalcOutput, CalcError> {
    let result = match operator.as_str() {
        "add" => left + right,
        "sub" => left - right,
        "mul" => left * right,
        "div" => left / right,
        other => return Err(CalcError::UnsupportedOperator(other.to_string())),
    };
    Ok(CalcOutput { result })
}

#[tokio::main]
async fn main() {
    // 1. Create a ToolRegistry and register the tool
    let mut registry = ToolRegistry::new();
    registry.register(CalculateTool);

    // 2. List all tool definitions
    let definitions = registry.definitions();
    println!("Registered tools:");
    for def in &definitions {
        println!("  - {} : {}", def.name, def.description);
    }

    // 3. Build a ToolContext (required by the execution pipeline)
    let ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "example-session".into(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
    };

    // 4. Execute the tool via the registry with JSON input
    let input = serde_json::json!({
        "left": 12.0,
        "right": 5.0,
        "operator": "mul"
    });

    let output = registry
        .execute("calculate", input, &ctx)
        .await
        .expect("tool execution should succeed");

    println!("\nTool output:");
    println!("  is_error: {}", output.is_error);
    for item in &output.content {
        if let neuron_types::ContentItem::Text(text) = item {
            println!("  content: {text}");
        }
    }
    if let Some(structured) = &output.structured_content {
        println!("  structured: {structured}");
    }

    // 5. Demonstrate direct typed call (bypassing registry)
    let typed_result = CalculateTool
        .call(
            CalculateArgs {
                left: 100.0,
                right: 7.0,
                operator: "div".into(),
            },
            &ctx,
        )
        .await
        .expect("typed call should succeed");

    println!("\nDirect typed call: 100 / 7 = {}", typed_result.result);
}
