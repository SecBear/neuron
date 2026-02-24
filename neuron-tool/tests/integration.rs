use neuron_tool::*;
use neuron_types::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PingArgs {
    _unused: Option<String>,
}

/// A fast tool that returns immediately.
struct FastTool;

impl Tool for FastTool {
    const NAME: &'static str = "fast_tool";
    type Args = PingArgs;
    type Output = String;
    type Error = std::convert::Infallible;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            title: None,
            description: "Returns immediately".into(),
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
        Ok("pong".to_string())
    }
}

/// A slow tool that sleeps for a configurable duration before returning.
struct SlowTool {
    delay: Duration,
}

impl SlowTool {
    fn new(delay: Duration) -> Self {
        Self { delay }
    }
}

impl Tool for SlowTool {
    const NAME: &'static str = "slow_tool";
    type Args = PingArgs;
    type Output = String;
    type Error = std::convert::Infallible;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            title: None,
            description: "Returns after a delay".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    fn call(
        &self,
        _args: Self::Args,
        _ctx: &ToolContext,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
        let delay = self.delay;
        async move {
            tokio::time::sleep(delay).await;
            Ok("finally done".to_string())
        }
    }
}

/// A simple passthrough tool used for structured output validation tests.
/// It accepts any JSON input and returns it as text output.
struct PassthroughTool;

impl ToolDyn for PassthroughTool {
    fn name(&self) -> &str {
        "result"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "result".into(),
            title: None,
            description: "Accepts structured output".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "age": { "type": "integer" }
                },
                "required": ["name", "age"]
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    fn call_dyn<'a>(
        &'a self,
        input: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Ok(ToolOutput {
                content: vec![ContentItem::Text(input.to_string())],
                structured_content: None,
                is_error: false,
            })
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

/// JSON Schema used for structured output validation tests.
fn person_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" }
        },
        "required": ["name", "age"]
    })
}

// ===========================================================================
// TimeoutMiddleware tests
// ===========================================================================

#[tokio::test]
async fn test_timeout_middleware_allows_fast_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(FastTool);
    registry.add_middleware(TimeoutMiddleware::new(Duration::from_secs(5)));

    let ctx = test_ctx();
    let result = registry
        .execute("fast_tool", serde_json::json!({}), &ctx)
        .await;

    let output = result.expect("fast tool should complete within timeout");
    assert!(!output.is_error);
    if let Some(ContentItem::Text(text)) = output.content.first() {
        assert_eq!(text, "pong");
    } else {
        panic!("expected text content from fast tool");
    }
}

#[tokio::test]
async fn test_timeout_middleware_rejects_slow_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(SlowTool::new(Duration::from_secs(5)));
    // Timeout of 50ms -- the slow tool sleeps for 5 seconds, so it will time out.
    registry.add_middleware(TimeoutMiddleware::new(Duration::from_millis(50)));

    let ctx = test_ctx();
    let result = registry
        .execute("slow_tool", serde_json::json!({}), &ctx)
        .await;

    match result {
        Err(ToolError::ExecutionFailed(err)) => {
            let msg = err.to_string();
            assert!(
                msg.contains("timed out"),
                "error should mention timeout, got: {msg}"
            );
            assert!(
                msg.contains("slow_tool"),
                "error should mention tool name, got: {msg}"
            );
        }
        other => panic!("expected ExecutionFailed from timeout, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_timeout_middleware_per_tool_override() {
    let mut registry = ToolRegistry::new();
    registry.register(FastTool);
    registry.register(SlowTool::new(Duration::from_millis(100)));

    // Default timeout: 10ms (too short for SlowTool)
    // Per-tool override for slow_tool: 2 seconds (plenty of time)
    registry.add_middleware(
        TimeoutMiddleware::new(Duration::from_millis(10))
            .with_tool_timeout("slow_tool", Duration::from_secs(2)),
    );

    let ctx = test_ctx();

    // fast_tool should still pass with the 10ms default
    let result = registry
        .execute("fast_tool", serde_json::json!({}), &ctx)
        .await;
    assert!(
        result.is_ok(),
        "fast tool should pass with default timeout: {result:?}"
    );

    // slow_tool should pass because of the per-tool override (2s > 100ms sleep)
    let result = registry
        .execute("slow_tool", serde_json::json!({}), &ctx)
        .await;
    assert!(
        result.is_ok(),
        "slow tool should pass with per-tool override: {result:?}"
    );
}

// ===========================================================================
// StructuredOutputValidator tests
// ===========================================================================

#[tokio::test]
async fn test_structured_output_validator_accepts_valid_input() {
    let mut registry = ToolRegistry::new();
    registry.register_dyn(std::sync::Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", StructuredOutputValidator::new(person_schema(), 3));

    let ctx = test_ctx();
    let result = registry
        .execute(
            "result",
            serde_json::json!({"name": "Alice", "age": 30}),
            &ctx,
        )
        .await;

    let output = result.expect("valid input should pass validation");
    assert!(!output.is_error);
    if let Some(ContentItem::Text(text)) = output.content.first() {
        assert!(
            text.contains("Alice"),
            "output should contain the input data: {text}"
        );
    } else {
        panic!("expected text content");
    }
}

#[tokio::test]
async fn test_structured_output_validator_rejects_invalid_input() {
    let mut registry = ToolRegistry::new();
    registry.register_dyn(std::sync::Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", StructuredOutputValidator::new(person_schema(), 3));

    let ctx = test_ctx();
    // Pass a string where schema expects an integer for "age"
    let result = registry
        .execute(
            "result",
            serde_json::json!({"name": "Alice", "age": "not a number"}),
            &ctx,
        )
        .await;

    match result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("validation failed") || msg.contains("Validation"),
                "error should mention validation failure, got: {msg}"
            );
            assert!(
                msg.contains("age"),
                "error should mention the invalid field, got: {msg}"
            );
        }
        other => panic!("expected ModelRetry for invalid input, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_structured_output_validator_missing_required_field() {
    let mut registry = ToolRegistry::new();
    registry.register_dyn(std::sync::Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", StructuredOutputValidator::new(person_schema(), 3));

    let ctx = test_ctx();
    // Missing the required "age" field
    let result = registry
        .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
        .await;

    match result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("age"),
                "error should mention the missing required field 'age', got: {msg}"
            );
            assert!(
                msg.contains("fix the output"),
                "error should ask the model to fix the output, got: {msg}"
            );
        }
        other => panic!("expected ModelRetry for missing required field, got: {other:?}"),
    }
}

// ===========================================================================
// RetryLimitedValidator tests
// ===========================================================================

#[tokio::test]
async fn test_retry_limited_validator_allows_retries() {
    let validator = StructuredOutputValidator::new(person_schema(), 3);
    let limited = RetryLimitedValidator::new(validator);

    let mut registry = ToolRegistry::new();
    registry.register_dyn(std::sync::Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", limited);

    let ctx = test_ctx();

    // First invalid attempt (attempt 1 of 3) -- should get ModelRetry
    let result = registry
        .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
        .await;
    match &result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("attempt 1/3"),
                "should show attempt 1/3, got: {msg}"
            );
        }
        other => panic!("expected ModelRetry on first bad attempt, got: {other:?}"),
    }

    // Second invalid attempt (attempt 2 of 3) -- should still get ModelRetry
    let result = registry
        .execute("result", serde_json::json!({"name": "Bob"}), &ctx)
        .await;
    match &result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("attempt 2/3"),
                "should show attempt 2/3, got: {msg}"
            );
        }
        other => panic!("expected ModelRetry on second bad attempt, got: {other:?}"),
    }

    // Third invalid attempt (attempt 3 of 3) -- should still get ModelRetry
    let result = registry
        .execute("result", serde_json::json!({"name": "Charlie"}), &ctx)
        .await;
    match &result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("attempt 3/3"),
                "should show attempt 3/3, got: {msg}"
            );
        }
        other => panic!("expected ModelRetry on third bad attempt, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_retry_limited_validator_exceeds_limit() {
    let validator = StructuredOutputValidator::new(person_schema(), 2);
    let limited = RetryLimitedValidator::new(validator);

    let mut registry = ToolRegistry::new();
    registry.register_dyn(std::sync::Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", limited);

    let ctx = test_ctx();

    // First two attempts: ModelRetry (within the 2-retry limit)
    let result = registry
        .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
        .await;
    assert!(
        matches!(&result, Err(ToolError::ModelRetry(_))),
        "attempt 1 should be ModelRetry: {result:?}"
    );

    let result = registry
        .execute("result", serde_json::json!({"name": "Bob"}), &ctx)
        .await;
    assert!(
        matches!(&result, Err(ToolError::ModelRetry(_))),
        "attempt 2 should be ModelRetry: {result:?}"
    );

    // Third attempt: exceeds max_retries=2, should become InvalidInput
    let result = registry
        .execute("result", serde_json::json!({"name": "Charlie"}), &ctx)
        .await;
    match result {
        Err(ToolError::InvalidInput(msg)) => {
            assert!(
                msg.contains("after 2 retries"),
                "error should mention the retry limit, got: {msg}"
            );
        }
        other => panic!("expected InvalidInput after exceeding retry limit, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_retry_limited_validator_resets_on_success() {
    let validator = StructuredOutputValidator::new(person_schema(), 2);
    let limited = RetryLimitedValidator::new(validator);

    let mut registry = ToolRegistry::new();
    registry.register_dyn(std::sync::Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", limited);

    let ctx = test_ctx();

    // First bad attempt
    let result = registry
        .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
        .await;
    assert!(
        matches!(&result, Err(ToolError::ModelRetry(_))),
        "first bad attempt should be ModelRetry: {result:?}"
    );

    // Successful attempt -- should reset the counter
    let result = registry
        .execute(
            "result",
            serde_json::json!({"name": "Alice", "age": 30}),
            &ctx,
        )
        .await;
    assert!(result.is_ok(), "valid input should pass: {result:?}");

    // Another bad attempt after reset -- counter should start from 0 again
    let result = registry
        .execute("result", serde_json::json!({"name": "Bob"}), &ctx)
        .await;
    match &result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("attempt 1/2"),
                "counter should have reset; expected attempt 1/2, got: {msg}"
            );
        }
        other => panic!("expected ModelRetry after counter reset, got: {other:?}"),
    }
}

// ===========================================================================
// TimeoutMiddleware — additional tests
// ===========================================================================

#[tokio::test]
async fn test_timeout_middleware_zero_duration() {
    let mut registry = ToolRegistry::new();
    // Use a tool with a tiny sleep — Duration::ZERO timeout should fire before
    // the sleep completes since tokio::time::timeout(ZERO) expires immediately
    // when the future yields.
    registry.register(SlowTool::new(Duration::from_millis(10)));
    registry.add_middleware(TimeoutMiddleware::new(Duration::ZERO));

    let ctx = test_ctx();
    let result = registry
        .execute("slow_tool", serde_json::json!({}), &ctx)
        .await;

    match result {
        Err(ToolError::ExecutionFailed(err)) => {
            let msg = err.to_string();
            assert!(
                msg.contains("timed out"),
                "error should mention timeout, got: {msg}"
            );
            assert!(
                msg.contains("slow_tool"),
                "error should mention tool name, got: {msg}"
            );
        }
        other => panic!("expected ExecutionFailed from zero-duration timeout, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_timeout_middleware_default_fallback() {
    let mut registry = ToolRegistry::new();
    registry.register(FastTool);

    // Set a per-tool override for a DIFFERENT tool (not fast_tool).
    // fast_tool should use the generous 10s default.
    registry.add_middleware(
        TimeoutMiddleware::new(Duration::from_secs(10))
            .with_tool_timeout("some_other_tool", Duration::from_millis(1)),
    );

    let ctx = test_ctx();
    let result = registry
        .execute("fast_tool", serde_json::json!({}), &ctx)
        .await;

    let output = result.expect("fast tool should succeed with 10s default fallback");
    assert!(!output.is_error);
    if let Some(ContentItem::Text(text)) = output.content.first() {
        assert_eq!(text, "pong");
    } else {
        panic!("expected text content from fast tool");
    }
}

#[tokio::test]
async fn test_timeout_middleware_error_message_format() {
    let mut registry = ToolRegistry::new();
    registry.register(SlowTool::new(Duration::from_secs(10)));
    // 2.5 second timeout — formatted as "2.5s" in the error message
    registry.add_middleware(TimeoutMiddleware::new(Duration::from_millis(2500)));

    let ctx = test_ctx();
    let result = registry
        .execute("slow_tool", serde_json::json!({}), &ctx)
        .await;

    match result {
        Err(ToolError::ExecutionFailed(err)) => {
            let msg = err.to_string();
            assert!(
                msg.contains("2.5s"),
                "error message should contain '2.5s' formatted duration, got: {msg}"
            );
            assert!(
                msg.contains("slow_tool"),
                "error message should mention the tool name, got: {msg}"
            );
        }
        other => panic!("expected ExecutionFailed with formatted duration, got: {other:?}"),
    }
}

/// Helper permission policy that allows everything.
struct AllowAll;

impl PermissionPolicy for AllowAll {
    fn check(&self, _tool_name: &str, _input: &serde_json::Value) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

#[tokio::test]
async fn test_timeout_middleware_in_chain_with_permission() {
    let mut registry = ToolRegistry::new();
    registry.register(FastTool);

    // Add PermissionChecker first (runs first in chain), then TimeoutMiddleware.
    registry.add_middleware(PermissionChecker::new(AllowAll));
    registry.add_middleware(TimeoutMiddleware::new(Duration::from_secs(10)));

    let ctx = test_ctx();
    let result = registry
        .execute("fast_tool", serde_json::json!({}), &ctx)
        .await;

    let output = result.expect("fast tool should pass both permission and timeout middleware");
    assert!(!output.is_error);
    if let Some(ContentItem::Text(text)) = output.content.first() {
        assert_eq!(text, "pong");
    } else {
        panic!("expected text content from fast tool");
    }
}

// ===========================================================================
// StructuredOutputValidator — additional tests
// ===========================================================================

#[tokio::test]
async fn test_structured_output_validator_non_object_schema() {
    // Schema that is not "type": "object" — should pass through without validation
    let string_schema = serde_json::json!({
        "type": "string"
    });

    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", StructuredOutputValidator::new(string_schema, 3));

    let ctx = test_ctx();
    // Pass an object (which is NOT a string) — should still succeed because
    // the validator only applies structural checks for object schemas.
    let result = registry
        .execute(
            "result",
            serde_json::json!({"name": "Alice", "age": 30}),
            &ctx,
        )
        .await;

    let output = result.expect("non-object schema should not block object input");
    assert!(!output.is_error);
}

#[tokio::test]
async fn test_structured_output_validator_boolean_type_mismatch() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "active": { "type": "boolean" }
        },
        "required": ["active"]
    });

    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", StructuredOutputValidator::new(schema, 3));

    let ctx = test_ctx();
    // Pass a string where boolean is expected
    let result = registry
        .execute("result", serde_json::json!({"active": "yes"}), &ctx)
        .await;

    match result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("active"),
                "error should mention the invalid field 'active', got: {msg}"
            );
            assert!(
                msg.contains("boolean"),
                "error should mention expected type 'boolean', got: {msg}"
            );
        }
        other => panic!("expected ModelRetry for boolean type mismatch, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_structured_output_validator_array_type_mismatch() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "tags": { "type": "array" }
        },
        "required": ["tags"]
    });

    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", StructuredOutputValidator::new(schema, 3));

    let ctx = test_ctx();
    // Pass a string where array is expected
    let result = registry
        .execute("result", serde_json::json!({"tags": "not-an-array"}), &ctx)
        .await;

    match result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("tags"),
                "error should mention the invalid field 'tags', got: {msg}"
            );
            assert!(
                msg.contains("array"),
                "error should mention expected type 'array', got: {msg}"
            );
        }
        other => panic!("expected ModelRetry for array type mismatch, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_structured_output_validator_error_message_content() {
    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", StructuredOutputValidator::new(person_schema(), 3));

    let ctx = test_ctx();
    // Trigger a validation failure — missing required field
    let result = registry
        .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
        .await;

    match result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("Output validation failed"),
                "error should contain 'Output validation failed', got: {msg}"
            );
            assert!(
                msg.contains("fix the output to match the schema"),
                "error should contain 'fix the output to match the schema', got: {msg}"
            );
        }
        other => panic!("expected ModelRetry with specific message format, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_structured_output_validator_unlimited_retries() {
    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    // The standalone StructuredOutputValidator has no retry cap —
    // it always returns ModelRetry on failure, regardless of how many times.
    registry.add_tool_middleware("result", StructuredOutputValidator::new(person_schema(), 3));

    let ctx = test_ctx();

    // Call 10 times with invalid input — every single call should be ModelRetry
    for i in 0..10 {
        let result = registry
            .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
            .await;
        match &result {
            Err(ToolError::ModelRetry(_)) => {} // Expected
            other => panic!(
                "attempt {}: expected ModelRetry (standalone validator has no cap), got: {other:?}",
                i + 1
            ),
        }
    }
}

#[tokio::test]
async fn test_structured_output_validator_valid_after_invalid() {
    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", StructuredOutputValidator::new(person_schema(), 3));

    let ctx = test_ctx();

    // First call: invalid input — should get ModelRetry
    let result = registry
        .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
        .await;
    assert!(
        matches!(&result, Err(ToolError::ModelRetry(_))),
        "first call with invalid input should be ModelRetry: {result:?}"
    );

    // Second call: valid input — should succeed (no state corruption)
    let result = registry
        .execute(
            "result",
            serde_json::json!({"name": "Alice", "age": 30}),
            &ctx,
        )
        .await;
    let output = result.expect("valid input after invalid should succeed");
    assert!(!output.is_error);
    if let Some(ContentItem::Text(text)) = output.content.first() {
        assert!(
            text.contains("Alice"),
            "output should contain the input data: {text}"
        );
    } else {
        panic!("expected text content");
    }
}

// ===========================================================================
// RetryLimitedValidator — additional tests
// ===========================================================================

#[tokio::test]
async fn test_retry_limited_validator_zero_max_retries() {
    // max_retries = 0 means the very first invalid call should be InvalidInput
    let validator = StructuredOutputValidator::new(person_schema(), 0);
    let limited = RetryLimitedValidator::new(validator);

    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", limited);

    let ctx = test_ctx();

    // First invalid call — should immediately return InvalidInput (not ModelRetry)
    let result = registry
        .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
        .await;
    match result {
        Err(ToolError::InvalidInput(msg)) => {
            assert!(
                msg.contains("after 0 retries"),
                "error should mention '0 retries', got: {msg}"
            );
        }
        other => panic!("expected InvalidInput immediately with max_retries=0, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_retry_limited_validator_one_max_retry() {
    // max_retries = 1 means: first invalid -> ModelRetry, second invalid -> InvalidInput
    let validator = StructuredOutputValidator::new(person_schema(), 1);
    let limited = RetryLimitedValidator::new(validator);

    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", limited);

    let ctx = test_ctx();

    // First invalid call — should get ModelRetry (attempt 1/1)
    let result = registry
        .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
        .await;
    match &result {
        Err(ToolError::ModelRetry(msg)) => {
            assert!(
                msg.contains("attempt 1/1"),
                "should show attempt 1/1, got: {msg}"
            );
        }
        other => panic!("expected ModelRetry on first invalid attempt, got: {other:?}"),
    }

    // Second invalid call — should get InvalidInput (exceeded max_retries=1)
    let result = registry
        .execute("result", serde_json::json!({"name": "Bob"}), &ctx)
        .await;
    match result {
        Err(ToolError::InvalidInput(msg)) => {
            assert!(
                msg.contains("after 1 retries"),
                "error should mention '1 retries', got: {msg}"
            );
        }
        other => panic!("expected InvalidInput after exceeding max_retries=1, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_retry_limited_validator_error_message_includes_count() {
    let validator = StructuredOutputValidator::new(person_schema(), 2);
    let limited = RetryLimitedValidator::new(validator);

    let mut registry = ToolRegistry::new();
    registry.register_dyn(Arc::new(PassthroughTool));
    registry.add_tool_middleware("result", limited);

    let ctx = test_ctx();

    // Exhaust all retries (2 ModelRetry calls)
    for _ in 0..2 {
        let result = registry
            .execute("result", serde_json::json!({"name": "Alice"}), &ctx)
            .await;
        assert!(
            matches!(&result, Err(ToolError::ModelRetry(_))),
            "should be ModelRetry within retry limit: {result:?}"
        );
    }

    // Third call exceeds max_retries=2 — verify error message content
    let result = registry
        .execute("result", serde_json::json!({"name": "Charlie"}), &ctx)
        .await;
    match result {
        Err(ToolError::InvalidInput(msg)) => {
            assert!(
                msg.contains("after 2 retries"),
                "error should mention retry count '2 retries', got: {msg}"
            );
            assert!(
                msg.contains("Output validation failed"),
                "error should mention 'Output validation failed', got: {msg}"
            );
        }
        other => panic!(
            "expected InvalidInput with retry count info after exceeding limit, got: {other:?}"
        ),
    }
}
