//! Tests for the #[neuron_tool] derive macro.

use neuron_tool::neuron_tool;
use neuron_types::*;
use std::collections::HashMap;
use std::path::PathBuf;

// --- Test output/error types ---

#[derive(Debug, serde::Serialize)]
struct EchoOutput {
    echoed: String,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
enum EchoError {
    #[error("echo failed")]
    Failed,
}

#[derive(Debug, serde::Serialize)]
struct MathOutput {
    result: f64,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
enum MathError {
    #[error("invalid expression: {0}")]
    Invalid(String),
}

// --- Derive a basic tool ---

#[neuron_tool(name = "echo", description = "Echo text back")]
async fn echo(
    /// The text to echo
    text: String,
    _ctx: &ToolContext,
) -> Result<EchoOutput, EchoError> {
    Ok(EchoOutput { echoed: text })
}

// --- Derive a multi-arg tool ---

#[neuron_tool(name = "add", description = "Add two numbers")]
async fn add(
    /// First number
    a: f64,
    /// Second number
    b: f64,
    _ctx: &ToolContext,
) -> Result<MathOutput, MathError> {
    Ok(MathOutput { result: a + b })
}

fn test_ctx() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    }
}

#[tokio::test]
async fn derive_basic_tool() {
    let tool = EchoTool;
    assert_eq!(Tool::definition(&tool).name, "echo");
    assert_eq!(Tool::definition(&tool).description, "Echo text back");

    let result = tool
        .call(
            EchoArgs {
                text: "hello".into(),
            },
            &test_ctx(),
        )
        .await
        .unwrap();
    assert_eq!(result.echoed, "hello");
}

#[tokio::test]
async fn derive_multi_arg_tool() {
    let tool = AddTool;
    assert_eq!(Tool::definition(&tool).name, "add");

    let result = tool
        .call(AddArgs { a: 3.0, b: 4.0 }, &test_ctx())
        .await
        .unwrap();
    assert_eq!(result.result, 7.0);
}

#[tokio::test]
async fn derive_tool_registers_in_registry() {
    let mut registry = neuron_tool::ToolRegistry::new();
    registry.register(EchoTool);

    let ctx = test_ctx();
    let result = registry
        .execute("echo", serde_json::json!({"text": "world"}), &ctx)
        .await
        .unwrap();
    assert!(!result.is_error);
}

#[test]
fn derive_tool_schema_has_descriptions() {
    let def = Tool::definition(&EchoTool);
    let schema = &def.input_schema;
    // The schema should have a "text" property
    let props = schema["properties"].as_object().unwrap();
    assert!(props.contains_key("text"));
}
