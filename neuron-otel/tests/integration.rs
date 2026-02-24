use neuron_otel::{OtelConfig, OtelHook};
use neuron_types::{
    CompletionRequest, CompletionResponse, ContentItem, HookAction, HookEvent, Message,
    ObservabilityHook, StopReason, TokenUsage, ToolOutput,
};

/// Helper: build a minimal CompletionRequest for testing.
fn test_request() -> CompletionRequest {
    CompletionRequest {
        model: "test-model".to_string(),
        messages: vec![Message::user("hello")],
        ..Default::default()
    }
}

/// Helper: build a minimal CompletionResponse for testing.
fn test_response() -> CompletionResponse {
    CompletionResponse {
        id: "resp-1".to_string(),
        model: "test-model".to_string(),
        message: Message::assistant("world"),
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    }
}

/// Helper: build a minimal ToolOutput for testing.
fn test_tool_output() -> ToolOutput {
    ToolOutput {
        content: vec![ContentItem::Text("result".to_string())],
        structured_content: None,
        is_error: false,
    }
}

/// Asserts that `action` is `HookAction::Continue`.
fn assert_continue(action: &HookAction) {
    assert!(
        matches!(action, HookAction::Continue),
        "expected HookAction::Continue, got {action:?}"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_otel_hook_returns_continue_for_all_events() {
    let hook = OtelHook::default();

    // LoopIteration
    let action = hook
        .on_event(HookEvent::LoopIteration { turn: 0 })
        .await
        .expect("LoopIteration should not error");
    assert_continue(&action);

    // PreLlmCall
    let req = test_request();
    let action = hook
        .on_event(HookEvent::PreLlmCall { request: &req })
        .await
        .expect("PreLlmCall should not error");
    assert_continue(&action);

    // PostLlmCall
    let resp = test_response();
    let action = hook
        .on_event(HookEvent::PostLlmCall { response: &resp })
        .await
        .expect("PostLlmCall should not error");
    assert_continue(&action);

    // PreToolExecution
    let input = serde_json::json!({"query": "test"});
    let action = hook
        .on_event(HookEvent::PreToolExecution {
            tool_name: "my_tool",
            input: &input,
        })
        .await
        .expect("PreToolExecution should not error");
    assert_continue(&action);

    // PostToolExecution
    let output = test_tool_output();
    let action = hook
        .on_event(HookEvent::PostToolExecution {
            tool_name: "my_tool",
            output: &output,
        })
        .await
        .expect("PostToolExecution should not error");
    assert_continue(&action);

    // ContextCompaction
    let action = hook
        .on_event(HookEvent::ContextCompaction {
            old_tokens: 5000,
            new_tokens: 2000,
        })
        .await
        .expect("ContextCompaction should not error");
    assert_continue(&action);

    // SessionStart
    let action = hook
        .on_event(HookEvent::SessionStart {
            session_id: "sess-123",
        })
        .await
        .expect("SessionStart should not error");
    assert_continue(&action);

    // SessionEnd
    let action = hook
        .on_event(HookEvent::SessionEnd {
            session_id: "sess-123",
        })
        .await
        .expect("SessionEnd should not error");
    assert_continue(&action);
}

#[tokio::test]
async fn test_otel_config_default_no_capture() {
    let config = OtelConfig::default();
    assert!(
        !config.capture_input,
        "default capture_input should be false"
    );
    assert!(
        !config.capture_output,
        "default capture_output should be false"
    );
}

#[tokio::test]
async fn test_otel_hook_default_construction() {
    // OtelHook::default() should compile and not panic.
    let _hook = OtelHook::default();
}

// ---------------------------------------------------------------------------
// Additional OtelHook tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_otel_hook_new_with_custom_config() {
    // Create hook with both capture flags enabled and verify all events still
    // return Continue.
    let config = OtelConfig {
        capture_input: true,
        capture_output: true,
    };
    let hook = OtelHook::new(config);

    // LoopIteration
    let action = hook
        .on_event(HookEvent::LoopIteration { turn: 0 })
        .await
        .expect("LoopIteration should not error");
    assert_continue(&action);

    // PreLlmCall
    let req = test_request();
    let action = hook
        .on_event(HookEvent::PreLlmCall { request: &req })
        .await
        .expect("PreLlmCall should not error");
    assert_continue(&action);

    // PostLlmCall
    let resp = test_response();
    let action = hook
        .on_event(HookEvent::PostLlmCall { response: &resp })
        .await
        .expect("PostLlmCall should not error");
    assert_continue(&action);

    // PreToolExecution
    let input = serde_json::json!({"query": "test"});
    let action = hook
        .on_event(HookEvent::PreToolExecution {
            tool_name: "my_tool",
            input: &input,
        })
        .await
        .expect("PreToolExecution should not error");
    assert_continue(&action);

    // PostToolExecution
    let output = test_tool_output();
    let action = hook
        .on_event(HookEvent::PostToolExecution {
            tool_name: "my_tool",
            output: &output,
        })
        .await
        .expect("PostToolExecution should not error");
    assert_continue(&action);

    // ContextCompaction
    let action = hook
        .on_event(HookEvent::ContextCompaction {
            old_tokens: 1000,
            new_tokens: 500,
        })
        .await
        .expect("ContextCompaction should not error");
    assert_continue(&action);

    // SessionStart
    let action = hook
        .on_event(HookEvent::SessionStart {
            session_id: "sess-custom",
        })
        .await
        .expect("SessionStart should not error");
    assert_continue(&action);

    // SessionEnd
    let action = hook
        .on_event(HookEvent::SessionEnd {
            session_id: "sess-custom",
        })
        .await
        .expect("SessionEnd should not error");
    assert_continue(&action);
}

#[tokio::test]
async fn test_otel_hook_capture_input_enabled() {
    // Create hook with capture_input: true. Fire a PreToolExecution event with
    // input data. Verify returns Continue and does not panic.
    let hook = OtelHook::new(OtelConfig {
        capture_input: true,
        capture_output: false,
    });

    let input = serde_json::json!({
        "query": "search term",
        "limit": 10,
        "nested": {"key": "value"}
    });
    let action = hook
        .on_event(HookEvent::PreToolExecution {
            tool_name: "search",
            input: &input,
        })
        .await
        .expect("PreToolExecution with capture_input should not error");
    assert_continue(&action);

    // Also fire PreLlmCall which exercises the capture_input branch
    let req = test_request();
    let action = hook
        .on_event(HookEvent::PreLlmCall { request: &req })
        .await
        .expect("PreLlmCall with capture_input should not error");
    assert_continue(&action);
}

#[tokio::test]
async fn test_otel_hook_capture_output_enabled() {
    // Create hook with capture_output: true. Fire PostToolExecution and
    // PostLlmCall events. Verify returns Continue and does not panic.
    let hook = OtelHook::new(OtelConfig {
        capture_input: false,
        capture_output: true,
    });

    let output = test_tool_output();
    let action = hook
        .on_event(HookEvent::PostToolExecution {
            tool_name: "search",
            output: &output,
        })
        .await
        .expect("PostToolExecution with capture_output should not error");
    assert_continue(&action);

    // Also fire PostLlmCall which exercises the capture_output branch
    let resp = test_response();
    let action = hook
        .on_event(HookEvent::PostLlmCall { response: &resp })
        .await
        .expect("PostLlmCall with capture_output should not error");
    assert_continue(&action);
}

#[tokio::test]
async fn test_otel_hook_post_tool_execution_with_error() {
    // Fire PostToolExecution with is_error: true. Verify returns Continue.
    let hook = OtelHook::default();

    let error_output = ToolOutput {
        content: vec![ContentItem::Text("Error: file not found".to_string())],
        structured_content: None,
        is_error: true,
    };

    let action = hook
        .on_event(HookEvent::PostToolExecution {
            tool_name: "read_file",
            output: &error_output,
        })
        .await
        .expect("PostToolExecution with is_error should not error");
    assert_continue(&action);
}

#[tokio::test]
async fn test_otel_hook_context_compaction_event() {
    // Fire ContextCompaction with specific token counts and verify Continue.
    let hook = OtelHook::default();

    let action = hook
        .on_event(HookEvent::ContextCompaction {
            old_tokens: 1000,
            new_tokens: 500,
        })
        .await
        .expect("ContextCompaction should not error");
    assert_continue(&action);
}

#[test]
fn test_otel_config_debug_and_clone() {
    // Verify Debug and Clone derives work correctly.
    let config = OtelConfig {
        capture_input: true,
        capture_output: false,
    };
    let cloned = config.clone();

    // Debug representations should be identical
    assert_eq!(format!("{:?}", config), format!("{:?}", cloned));

    // Verify field values survived the clone
    assert!(cloned.capture_input);
    assert!(!cloned.capture_output);
}

#[tokio::test]
async fn test_otel_hook_high_turn_number() {
    // Fire LoopIteration with usize::MAX to verify no overflow or panic.
    let hook = OtelHook::default();

    let action = hook
        .on_event(HookEvent::LoopIteration { turn: usize::MAX })
        .await
        .expect("LoopIteration with usize::MAX should not error");
    assert_continue(&action);
}
