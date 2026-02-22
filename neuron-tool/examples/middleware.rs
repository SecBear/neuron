//! Middleware example: logging and auth middleware on tool calls.
//!
//! Run with: cargo run --example middleware -p neuron-tool

use std::collections::HashMap;
use std::path::PathBuf;

use neuron_tool::{ToolRegistry, neuron_tool, tool_middleware_fn};
use neuron_types::{ContentItem, ToolContext, ToolError};
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
    // 1. Create a registry and register the calculator tool
    let mut registry = ToolRegistry::new();
    registry.register(CalculateTool);

    // 2. Add a GLOBAL logging middleware (runs on every tool call)
    registry.add_middleware(tool_middleware_fn(|call, ctx, next| {
        Box::pin(async move {
            println!(
                "[log] >>> calling tool '{}' with input: {}",
                call.name, call.input
            );
            let result = next.run(call, ctx).await;
            match &result {
                Ok(output) => {
                    let text = output
                        .content
                        .iter()
                        .filter_map(|item| match item {
                            ContentItem::Text(t) => Some(t.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("[log] <<< tool '{}' succeeded: {text}", call.name);
                }
                Err(e) => {
                    println!("[log] <<< tool '{}' failed: {e}", call.name);
                }
            }
            result
        })
    }));

    // 3. Add a PER-TOOL auth middleware on "calculate"
    //    Checks for an "auth" key in the JSON input; rejects if missing.
    registry.add_tool_middleware(
        "calculate",
        tool_middleware_fn(|call, ctx, next| {
            Box::pin(async move {
                if call.input.get("auth").is_none() {
                    return Err(ToolError::PermissionDenied(
                        "missing 'auth' key in input".to_string(),
                    ));
                }
                println!("[auth] validated auth for tool '{}'", call.name);
                next.run(call, ctx).await
            })
        }),
    );

    // 4. Build a ToolContext
    let ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "middleware-example".into(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
    };

    // 5. Execute WITH auth — should succeed
    println!("=== Call with auth ===");
    let input_with_auth = serde_json::json!({
        "left": 10.0,
        "right": 3.0,
        "operator": "add",
        "auth": "token-abc123"
    });

    match registry.execute("calculate", input_with_auth, &ctx).await {
        Ok(output) => {
            println!("Result: is_error={}", output.is_error);
            for item in &output.content {
                if let ContentItem::Text(t) = item {
                    println!("  content: {t}");
                }
            }
        }
        Err(e) => println!("Error: {e}"),
    }

    // 6. Execute WITHOUT auth — should fail via middleware short-circuit
    println!("\n=== Call without auth ===");
    let input_no_auth = serde_json::json!({
        "left": 10.0,
        "right": 3.0,
        "operator": "add"
    });

    match registry.execute("calculate", input_no_auth, &ctx).await {
        Ok(output) => {
            println!("Result: is_error={}", output.is_error);
            for item in &output.content {
                if let ContentItem::Text(t) = item {
                    println!("  content: {t}");
                }
            }
        }
        Err(e) => println!("Error (expected): {e}"),
    }
}
