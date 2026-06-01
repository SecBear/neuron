//! Behavioral tests for [`PythonExecTool`].
//!
//! These drive the operator end-to-end against a real local `python3`
//! subprocess (via `LocalPythonBackend` + `InMemoryComputeRuntime`), exactly
//! like the other integration tests in this crate.

use std::sync::Arc;

use layer0::content::{Content, ContentBlock};
use layer0::dispatch_context::DispatchContext;
use layer0::id::{DispatchId, OperatorId};
use layer0::operator::{Operator, OperatorInput, TriggerType};

use skg_op_compute_runtime::python::LocalPythonBackend;
use skg_op_compute_runtime::runtime::InMemoryComputeRuntime;
use skg_op_compute_runtime::{ExecutionProfile, PythonExecTool};

/// Build a tool backed by the real local python backend.
fn build_tool() -> PythonExecTool {
    let runtime = Arc::new(InMemoryComputeRuntime::new(LocalPythonBackend, "python"));
    PythonExecTool::new(runtime, ExecutionProfile::default())
}

/// Build a `DispatchContext` whose dispatch id doubles as the default session key.
fn ctx(dispatch: &str) -> DispatchContext {
    DispatchContext::new(DispatchId::new(dispatch), OperatorId::new("python_exec"))
}

/// Wrap a JSON payload as text operator input (the tool parses text as JSON).
fn input(payload: serde_json::Value) -> OperatorInput {
    OperatorInput::new(Content::text(payload.to_string()), TriggerType::User)
}

#[tokio::test]
async fn final_result_preferred() {
    let tool = build_tool();
    let output = tool
        .handle(
            input(serde_json::json!({ "code": "final({\"answer\": 42})" })),
            &ctx("final-result"),
        )
        .await
        .expect("handle")
        .collect()
        .await
        .expect("collect");

    // The prelude `final(...)` recorded a structured result, so the tool returns
    // it as a JSON Data block rather than stdout.
    match &output.message {
        Content::Blocks(blocks) => {
            let data = blocks
                .iter()
                .find_map(|b| match b {
                    ContentBlock::Data { data, .. } => Some(data),
                    _ => None,
                })
                .expect("expected a data block carrying the final result");
            assert_eq!(data["answer"], serde_json::json!(42));
        }
        other => panic!("expected structured final result, got {other:?}"),
    }
}

#[tokio::test]
async fn stdout_when_no_final() {
    let tool = build_tool();
    let output = tool
        .handle(
            input(serde_json::json!({ "code": "print(\"hi\")" })),
            &ctx("stdout-only"),
        )
        .await
        .expect("handle")
        .collect()
        .await
        .expect("collect");

    // No `final(...)` was called, so the tool falls back to stdout text.
    let text = output
        .message
        .as_text()
        .expect("expected stdout text content");
    assert!(
        text.contains("hi"),
        "stdout should contain printed text, got {text:?}"
    );
}

#[tokio::test]
async fn missing_code_field_errors() {
    let tool = build_tool();
    let result = tool
        .handle(input(serde_json::json!({})), &ctx("missing-code"))
        .await;

    // The required `code` field is absent, so `handle` rejects the input with a
    // `ProtocolError` before any execution happens.
    assert!(result.is_err(), "missing 'code' field should error");
}
