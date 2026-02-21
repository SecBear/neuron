use agent_types::*;
use std::future::Future;
use std::time::Duration;

struct NoopHook;

impl ObservabilityHook for NoopHook {
    fn on_event(
        &self,
        _event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
        async { Ok(HookAction::Continue) }
    }
}

struct AllowAll;

impl PermissionPolicy for AllowAll {
    fn check(&self, _tool_name: &str, _input: &serde_json::Value) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

struct DenyBash;

impl PermissionPolicy for DenyBash {
    fn check(&self, tool_name: &str, _input: &serde_json::Value) -> PermissionDecision {
        if tool_name == "bash" {
            PermissionDecision::Deny("bash is not allowed".into())
        } else {
            PermissionDecision::Allow
        }
    }
}

#[tokio::test]
async fn noop_hook_continues() {
    let hook = NoopHook;
    let event = HookEvent::LoopIteration { turn: 1 };
    let action = hook.on_event(event).await.unwrap();
    assert!(matches!(action, HookAction::Continue));
}

#[tokio::test]
async fn hook_skip_variant() {
    let action = HookAction::Skip { reason: "blocked".into() };
    if let HookAction::Skip { reason } = action {
        assert_eq!(reason, "blocked");
    } else {
        panic!("expected Skip");
    }
}

#[tokio::test]
async fn hook_terminate_variant() {
    let action = HookAction::Terminate { reason: "too many turns".into() };
    if let HookAction::Terminate { reason } = action {
        assert_eq!(reason, "too many turns");
    } else {
        panic!("expected Terminate");
    }
}

#[test]
fn allow_all_policy() {
    let policy = AllowAll;
    let decision = policy.check("bash", &serde_json::json!({"cmd": "ls"}));
    assert!(matches!(decision, PermissionDecision::Allow));
}

#[test]
fn deny_bash_policy() {
    let policy = DenyBash;
    let decision = policy.check("bash", &serde_json::json!({"cmd": "rm -rf /"}));
    assert!(matches!(decision, PermissionDecision::Deny(_)));
    let decision = policy.check("read_file", &serde_json::json!({}));
    assert!(matches!(decision, PermissionDecision::Allow));
}

#[test]
fn hook_event_variants() {
    // Verify all HookEvent variants can be constructed
    let _ = HookEvent::LoopIteration { turn: 0 };
    let req = CompletionRequest {
        model: "m".into(),
        messages: vec![],
        system: None,
        tools: vec![],
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };
    let _ = HookEvent::PreLlmCall { request: &req };
    let resp = CompletionResponse {
        id: "id".into(),
        model: "m".into(),
        message: Message { role: Role::Assistant, content: vec![] },
        usage: TokenUsage::default(),
        stop_reason: StopReason::EndTurn,
    };
    let _ = HookEvent::PostLlmCall { response: &resp };
    let val = serde_json::json!({});
    let _ = HookEvent::PreToolExecution { tool_name: "t", input: &val };
    let out = ToolOutput { content: vec![], structured_content: None, is_error: false };
    let _ = HookEvent::PostToolExecution { tool_name: "t", output: &out };
    let _ = HookEvent::ContextCompaction { old_tokens: 100, new_tokens: 50 };
    let _ = HookEvent::SessionStart { session_id: "s" };
    let _ = HookEvent::SessionEnd { session_id: "s" };
}

#[test]
fn activity_options_construction() {
    let opts = ActivityOptions {
        start_to_close_timeout: Duration::from_secs(30),
        heartbeat_timeout: Some(Duration::from_secs(5)),
        retry_policy: Some(RetryPolicy {
            initial_interval: Duration::from_millis(100),
            backoff_coefficient: 2.0,
            maximum_attempts: 3,
            maximum_interval: Duration::from_secs(10),
            non_retryable_errors: vec!["auth".into()],
        }),
    };
    assert_eq!(opts.start_to_close_timeout, Duration::from_secs(30));
    assert_eq!(opts.retry_policy.unwrap().maximum_attempts, 3);
}

#[test]
fn permission_decision_ask_variant() {
    let decision = PermissionDecision::Ask("confirm dangerous operation".into());
    if let PermissionDecision::Ask(msg) = decision {
        assert!(msg.contains("dangerous"));
    } else {
        panic!("expected Ask");
    }
}
