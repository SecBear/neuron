use neuron_tool::*;
use neuron_types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// Additional tool that always fails, used in error propagation tests.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FailArgs {
    _unused: Option<String>,
}

struct AlwaysFailTool;

impl Tool for AlwaysFailTool {
    const NAME: &'static str = "always_fail";
    type Args = FailArgs;
    type Output = String;
    type Error = AlwaysFailError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            title: None,
            description: "Always fails".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(
        &self,
        _args: Self::Args,
        _ctx: &ToolContext,
    ) -> Result<Self::Output, Self::Error> {
        Err(AlwaysFailError::Boom)
    }
}

#[derive(Debug, thiserror::Error)]
enum AlwaysFailError {
    #[error("boom")]
    Boom,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReadFileArgs {
    path: String,
}

#[derive(Debug, Serialize)]
struct ReadFileOutput {
    content: String,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
enum ReadFileError {
    #[error("not found: {0}")]
    NotFound(String),
}

struct ReadFileTool;

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Args = ReadFileArgs;
    type Output = ReadFileOutput;
    type Error = ReadFileError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            title: None,
            description: "Read a file".into(),
            input_schema: serde_json::to_value(schemars::schema_for!(ReadFileArgs)).unwrap(),
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
        Ok(ReadFileOutput {
            content: format!("contents of {}", args.path),
        })
    }
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
async fn global_middleware_wraps_all_tools() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    registry.add_middleware(tool_middleware_fn(move |call, ctx, next| {
        let c = counter_clone.clone();
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
            next.run(call, ctx).await
        })
    }));

    let ctx = test_ctx();
    registry
        .execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx)
        .await
        .unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn per_tool_middleware_only_applies_to_named_tool() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    registry.add_tool_middleware(
        "read_file",
        tool_middleware_fn(move |call, ctx, next| {
            let c = counter_clone.clone();
            Box::pin(async move {
                c.fetch_add(1, Ordering::SeqCst);
                next.run(call, ctx).await
            })
        }),
    );

    let ctx = test_ctx();
    registry
        .execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx)
        .await
        .unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn middleware_can_short_circuit() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    registry.add_middleware(tool_middleware_fn(|_call, _ctx, _next| {
        Box::pin(async {
            // Don't call next — short-circuit
            Ok(ToolOutput {
                content: vec![ContentItem::Text("blocked".into())],
                structured_content: None,
                is_error: true,
            })
        })
    }));

    let ctx = test_ctx();
    let result = registry
        .execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx)
        .await
        .unwrap();
    assert!(result.is_error);
}

#[tokio::test]
async fn middleware_ordering_global_before_per_tool() {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));

    let order1 = order.clone();
    let order2 = order.clone();

    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    registry.add_middleware(tool_middleware_fn(move |call, ctx, next| {
        let o = order1.clone();
        Box::pin(async move {
            o.lock().unwrap().push("global");
            next.run(call, ctx).await
        })
    }));
    registry.add_tool_middleware(
        "read_file",
        tool_middleware_fn(move |call, ctx, next| {
            let o = order2.clone();
            Box::pin(async move {
                o.lock().unwrap().push("per_tool");
                next.run(call, ctx).await
            })
        }),
    );

    let ctx = test_ctx();
    registry
        .execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx)
        .await
        .unwrap();

    let executed = order.lock().unwrap();
    assert_eq!(&*executed, &["global", "per_tool"]);
}

// --- Error propagation through middleware ---

#[tokio::test]
async fn tool_execution_error_propagates_through_middleware() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let mut registry = ToolRegistry::new();
    registry.register(AlwaysFailTool);
    registry.add_middleware(tool_middleware_fn(move |call, ctx, next| {
        let c = counter_clone.clone();
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
            // Middleware runs, then the tool fails — error should propagate back
            next.run(call, ctx).await
        })
    }));

    let ctx = test_ctx();
    let result = registry
        .execute("always_fail", serde_json::json!({}), &ctx)
        .await;

    // Middleware was invoked
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    // Tool error propagated through
    match result {
        Err(ToolError::ExecutionFailed(_)) => {} // expected
        other => panic!("expected ExecutionFailed error, got: {other:?}"),
    }
}

#[tokio::test]
async fn middleware_itself_returning_error() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    registry.add_middleware(tool_middleware_fn(|_call, _ctx, _next| {
        Box::pin(async {
            // Middleware returns an error without calling next
            Err(ToolError::InvalidInput(
                "middleware rejected this call".into(),
            ))
        })
    }));

    let ctx = test_ctx();
    let result = registry
        .execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx)
        .await;

    match result {
        Err(ToolError::InvalidInput(msg)) => {
            assert!(
                msg.contains("middleware rejected"),
                "expected middleware error message, got: {msg}"
            );
        }
        other => panic!("expected InvalidInput error from middleware, got: {other:?}"),
    }
}

#[tokio::test]
async fn schema_validator_skips_tool_not_in_snapshot() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);

    // Create SchemaValidator BEFORE registering AlwaysFailTool
    registry.add_middleware(SchemaValidator::new(&registry));

    // Register a second tool AFTER the validator snapshot
    registry.register(AlwaysFailTool);

    let ctx = test_ctx();

    // The validator doesn't have "always_fail" in its snapshot,
    // so it should skip validation and let the call through.
    // The tool itself will fail, but that's after validation.
    let result = registry
        .execute("always_fail", serde_json::json!({}), &ctx)
        .await;

    // Should NOT be InvalidInput — should be ExecutionFailed from the tool itself
    match result {
        Err(ToolError::ExecutionFailed(_)) => {} // expected — validator skipped, tool ran and failed
        other => {
            panic!("expected ExecutionFailed (validator should skip unknown tools), got: {other:?}")
        }
    }

    // Meanwhile, the snapshotted tool should still be validated
    let result = registry
        .execute("read_file", serde_json::json!({}), &ctx)
        .await;
    match result {
        Err(ToolError::InvalidInput(msg)) => {
            assert!(
                msg.contains("path"),
                "should still validate known tools: {msg}"
            );
        }
        other => panic!("expected InvalidInput for read_file missing 'path', got: {other:?}"),
    }
}
