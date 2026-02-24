//! Integration tests for neuron-loop.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::stream;
use neuron_context::SlidingWindowStrategy;
use neuron_loop::{AgentLoop, LoopConfig};
use neuron_tool::ToolRegistry;
use neuron_types::{
    ActivityOptions, CompletionRequest, CompletionResponse, ContentBlock, ContentItem,
    ContextError, ContextStrategy, DurableContext, DurableError, HookAction, HookError, HookEvent,
    LoopError, Message, ObservabilityHook, ProviderError, Role, StopReason, StreamEvent,
    StreamHandle, SystemPrompt, TokenUsage, Tool, ToolContext, ToolDefinition, ToolDyn, ToolError,
    ToolOutput, UsageLimits, WasmBoxedFuture,
};
use tokio_util::sync::CancellationToken;

/// A mock provider that returns pre-configured responses in sequence.
struct MockProvider {
    responses: Mutex<Vec<CompletionResponse>>,
}

impl MockProvider {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl neuron_types::Provider for MockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        let response = {
            let mut responses = self.responses.lock().expect("test lock poisoned");
            if responses.is_empty() {
                panic!("MockProvider: no more responses configured");
            }
            responses.remove(0)
        };
        async move { Ok(response) }
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest(
            "streaming not implemented in mock".into(),
        ))
    }
}

/// A mock tool that echoes its input.
struct EchoTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EchoArgs {
    text: String,
}

impl Tool for EchoTool {
    const NAME: &'static str = "echo";
    type Args = EchoArgs;
    type Output = String;
    type Error = std::io::Error;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            title: Some("Echo".to_string()),
            description: "Echoes input text".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(&self, args: EchoArgs, _ctx: &ToolContext) -> Result<String, std::io::Error> {
        Ok(format!("echo: {}", args.text))
    }
}

/// Helper to create a default ToolContext for tests.
fn test_tool_context() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test-session".to_string(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
    }
}

/// Helper to create a simple text CompletionResponse.
fn text_response(text: &str) -> CompletionResponse {
    CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text(text.to_string())],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    }
}

/// Helper to create a tool_use CompletionResponse.
fn tool_use_response(
    tool_id: &str,
    tool_name: &str,
    input: serde_json::Value,
) -> CompletionResponse {
    CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: tool_id.to_string(),
                name: tool_name.to_string(),
                input,
            }],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason: StopReason::ToolUse,
    }
}

// --- Task 5.2 tests ---

#[test]
fn test_loop_config_defaults() {
    let config = LoopConfig::default();
    assert!(config.max_turns.is_none());
    assert!(!config.parallel_tool_execution);
    match &config.system_prompt {
        SystemPrompt::Text(text) => assert!(text.is_empty()),
        _ => panic!("expected Text variant for default system prompt"),
    }
}

#[test]
fn test_neuron_loop_construction() {
    let provider = MockProvider::new(vec![]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        system_prompt: SystemPrompt::Text("You are a helpful assistant.".to_string()),
        max_turns: Some(5),
        parallel_tool_execution: false,
        ..LoopConfig::default()
    };

    let agent = AgentLoop::new(provider, tools, context, config);

    assert_eq!(agent.config().max_turns, Some(5));
    assert!(!agent.config().parallel_tool_execution);
    assert!(agent.messages().is_empty());
}

// --- Task 5.3 tests ---

#[tokio::test]
async fn test_run_text_only_completes_in_one_turn() {
    let provider = MockProvider::new(vec![text_response("Hello, world!")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        system_prompt: SystemPrompt::Text("You are helpful.".to_string()),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    assert_eq!(result.response, "Hello, world!");
    assert_eq!(result.turns, 1);
    assert_eq!(result.usage.input_tokens, 10);
    assert_eq!(result.usage.output_tokens, 5);
}

#[tokio::test]
async fn test_run_with_tool_call_completes_in_two_turns() {
    // First response: model calls echo tool
    // Second response: model returns final text
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("The echo says: echo: hello"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo hello".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    assert_eq!(result.response, "The echo says: echo: hello");
    assert_eq!(result.turns, 2);
    // Cumulative usage: 10+10 input, 5+5 output
    assert_eq!(result.usage.input_tokens, 20);
    assert_eq!(result.usage.output_tokens, 10);
}

#[tokio::test]
async fn test_run_max_turns_limit() {
    // Provider always returns tool calls — should be stopped by max_turns
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "1"})),
        tool_use_response("call-2", "echo", serde_json::json!({"text": "2"})),
        tool_use_response("call-3", "echo", serde_json::json!({"text": "3"})),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        max_turns: Some(2),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Keep echoing".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::MaxTurns(n) => assert_eq!(n, 2),
        other => panic!("expected MaxTurns error, got: {other:?}"),
    }
}

// --- Task 5.4 tests ---

/// A hook that records which events it receives.
struct RecordingHook {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingHook {
    fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                events: events.clone(),
            },
            events,
        )
    }
}

impl ObservabilityHook for RecordingHook {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
        let label = match &event {
            HookEvent::PreLlmCall { .. } => "PreLlmCall",
            HookEvent::PostLlmCall { .. } => "PostLlmCall",
            HookEvent::PreToolExecution { tool_name, .. } => {
                let name = format!("PreToolExecution:{tool_name}");
                self.events.lock().expect("lock").push(name);
                return std::future::ready(Ok(HookAction::Continue));
            }
            HookEvent::PostToolExecution { tool_name, .. } => {
                let name = format!("PostToolExecution:{tool_name}");
                self.events.lock().expect("lock").push(name);
                return std::future::ready(Ok(HookAction::Continue));
            }
            HookEvent::ContextCompaction { .. } => "ContextCompaction",
            HookEvent::LoopIteration { .. } => "LoopIteration",
            HookEvent::SessionStart { .. } => "SessionStart",
            HookEvent::SessionEnd { .. } => "SessionEnd",
        };
        self.events.lock().expect("lock").push(label.to_string());
        std::future::ready(Ok(HookAction::Continue))
    }
}

#[tokio::test]
async fn test_hooks_receive_pre_and_post_llm_events() {
    let provider = MockProvider::new(vec![text_response("Hi there")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    let recorded = events.lock().expect("lock");
    assert!(recorded.contains(&"PreLlmCall".to_string()));
    assert!(recorded.contains(&"PostLlmCall".to_string()));
}

#[tokio::test]
async fn test_hooks_receive_tool_execution_events() {
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("Done"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo".to_string())],
    };

    agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    let recorded = events.lock().expect("lock");
    assert!(recorded.contains(&"PreToolExecution:echo".to_string()));
    assert!(recorded.contains(&"PostToolExecution:echo".to_string()));
}

/// A hook that terminates the loop on PreLlmCall.
struct TerminatingHook;

impl ObservabilityHook for TerminatingHook {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
        let action = match event {
            HookEvent::PreLlmCall { .. } => HookAction::Terminate {
                reason: "test termination".to_string(),
            },
            _ => HookAction::Continue,
        };
        std::future::ready(Ok(action))
    }
}

#[tokio::test]
async fn test_hook_terminate_stops_loop() {
    let provider = MockProvider::new(vec![text_response("Should not reach this")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    agent.add_hook(TerminatingHook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::HookTerminated(reason) => assert_eq!(reason, "test termination"),
        other => panic!("expected HookTerminated, got: {other:?}"),
    }
}

/// A hook that skips tool execution.
struct SkipToolHook;

impl ObservabilityHook for SkipToolHook {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
        let action = match event {
            HookEvent::PreToolExecution { .. } => HookAction::Skip {
                reason: "tool blocked by policy".to_string(),
            },
            _ => HookAction::Continue,
        };
        std::future::ready(Ok(action))
    }
}

#[tokio::test]
async fn test_hook_skip_returns_rejection_message() {
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("OK, tool was skipped"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    agent.add_hook(SkipToolHook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");
    assert_eq!(result.response, "OK, tool was skipped");

    // Check that the tool result message contains the skip reason
    let tool_result_msg = result
        .messages
        .iter()
        .find(|m| {
            m.content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { is_error: true, .. }))
        })
        .expect("should have a tool result message with error");

    let has_skip_text = tool_result_msg.content.iter().any(|b| {
        if let ContentBlock::ToolResult {
            content, is_error, ..
        } = b
        {
            *is_error
                && content.iter().any(|c| {
                    if let ContentItem::Text(t) = c {
                        t.contains("tool blocked by policy")
                    } else {
                        false
                    }
                })
        } else {
            false
        }
    });
    assert!(has_skip_text, "tool result should contain skip reason");
}

// --- Task 5.5 tests ---

#[tokio::test]
async fn test_context_compaction_triggered_by_token_threshold() {
    // Use a very low max_tokens so compaction triggers after a few messages.
    // SlidingWindowStrategy with window_size=2 and max_tokens=50
    // The TokenCounter estimates ~4 tokens per word, so a few messages will exceed 50.

    // The provider will return tool calls for 3 turns, then text.
    // After the first couple of turns, context should be compacted.
    let provider = MockProvider::new(vec![
        tool_use_response(
            "call-1",
            "echo",
            serde_json::json!({"text": "first message with enough words to generate tokens"}),
        ),
        tool_use_response(
            "call-2",
            "echo",
            serde_json::json!({"text": "second message with even more words for token counting"}),
        ),
        text_response("Final answer after compaction"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    // Very low max_tokens to force compaction, window_size of 2
    let context = SlidingWindowStrategy::new(2, 50);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text(
            "Start with a reasonably long message so tokens accumulate quickly for testing"
                .to_string(),
        )],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    assert_eq!(result.response, "Final answer after compaction");

    // Verify the ContextCompaction hook event was fired
    let recorded = events.lock().expect("lock");
    assert!(
        recorded.contains(&"ContextCompaction".to_string()),
        "expected ContextCompaction event, got: {recorded:?}"
    );
}

// --- Task 5.6 tests ---

/// A mock DurableContext that records calls and delegates to provider/tool.
struct MockDurable {
    llm_calls: Arc<Mutex<Vec<String>>>,
    tool_calls: Arc<Mutex<Vec<String>>>,
    /// Pre-configured LLM responses (same as MockProvider).
    llm_responses: Mutex<Vec<CompletionResponse>>,
}

/// Alias for a shared log of call names.
type CallLog = Arc<Mutex<Vec<String>>>;

impl MockDurable {
    fn new(llm_responses: Vec<CompletionResponse>) -> (Self, CallLog, CallLog) {
        let llm_calls = Arc::new(Mutex::new(Vec::new()));
        let tool_calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                llm_calls: llm_calls.clone(),
                tool_calls: tool_calls.clone(),
                llm_responses: Mutex::new(llm_responses),
            },
            llm_calls,
            tool_calls,
        )
    }
}

impl DurableContext for MockDurable {
    fn execute_llm_call(
        &self,
        _request: CompletionRequest,
        _options: ActivityOptions,
    ) -> impl Future<Output = Result<CompletionResponse, DurableError>> + Send {
        self.llm_calls
            .lock()
            .expect("lock")
            .push("execute_llm_call".to_string());
        let response = {
            let mut responses = self.llm_responses.lock().expect("lock");
            if responses.is_empty() {
                return std::future::ready(Err(DurableError::ActivityFailed(
                    "no more responses".into(),
                )));
            }
            responses.remove(0)
        };
        std::future::ready(Ok(response))
    }

    fn execute_tool(
        &self,
        tool_name: &str,
        _input: serde_json::Value,
        _ctx: &ToolContext,
        _options: ActivityOptions,
    ) -> impl Future<Output = Result<ToolOutput, DurableError>> + Send {
        let name = tool_name.to_string();
        self.tool_calls
            .lock()
            .expect("lock")
            .push(format!("execute_tool:{name}"));
        std::future::ready(Ok(ToolOutput {
            content: vec![ContentItem::Text(format!("durable result for {name}"))],
            structured_content: None,
            is_error: false,
        }))
    }

    fn wait_for_signal<T: serde::de::DeserializeOwned + Send>(
        &self,
        _signal_name: &str,
        _timeout: std::time::Duration,
    ) -> impl Future<Output = Result<Option<T>, DurableError>> + Send {
        std::future::ready(Ok(None))
    }

    fn should_continue_as_new(&self) -> bool {
        false
    }

    fn continue_as_new(
        &self,
        _state: serde_json::Value,
    ) -> impl Future<Output = Result<(), DurableError>> + Send {
        std::future::ready(Ok(()))
    }

    fn sleep(&self, _duration: std::time::Duration) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }

    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }
}

#[tokio::test]
async fn test_durable_context_routes_llm_calls() {
    // Provider should NOT be called when durability is set
    let provider = MockProvider::new(vec![]); // Empty — would panic if called

    let (durable, llm_calls, tool_calls) =
        MockDurable::new(vec![text_response("Durable response")]);

    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    agent.set_durability(durable);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");
    assert_eq!(result.response, "Durable response");

    let llm = llm_calls.lock().expect("lock");
    assert_eq!(llm.len(), 1);
    assert_eq!(llm[0], "execute_llm_call");

    let tools = tool_calls.lock().expect("lock");
    assert!(tools.is_empty());
}

#[tokio::test]
async fn test_durable_context_routes_tool_calls() {
    let provider = MockProvider::new(vec![]); // Empty — would panic if called

    let (durable, llm_calls, tool_calls) = MockDurable::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("Done via durable"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    agent.set_durability(durable);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");
    assert_eq!(result.response, "Done via durable");

    let llm = llm_calls.lock().expect("lock");
    assert_eq!(llm.len(), 2); // Two LLM calls through durable

    let tools = tool_calls.lock().expect("lock");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0], "execute_tool:echo");
}

#[tokio::test]
async fn test_without_durability_calls_provider_directly() {
    // Without durability, provider is called directly
    let provider = MockProvider::new(vec![text_response("Direct response")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    // No durability set

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");
    assert_eq!(result.response, "Direct response");
}

// --- Task 5.7 tests ---

use neuron_loop::TurnResult;

#[tokio::test]
async fn test_run_step_yields_turn_results() {
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "step1"})),
        text_response("Final step response"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go step by step".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    // First turn: tool execution
    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::ToolsExecuted { calls, results } => {
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].1, "echo");
            assert_eq!(results.len(), 1);
        }
        other => panic!("expected ToolsExecuted, got: {other:?}"),
    }

    // Can inspect messages between turns
    assert!(iter.messages().len() >= 3); // user + assistant + tool_result

    // Second turn: final response
    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::FinalResponse(agent_result) => {
            assert_eq!(agent_result.response, "Final step response");
            assert_eq!(agent_result.turns, 2);
        }
        other => panic!("expected FinalResponse, got: {other:?}"),
    }

    // No more turns
    assert!(iter.next().await.is_none());
}

#[tokio::test]
async fn test_run_step_inject_message() {
    // Provider returns text after seeing injected message
    let provider = MockProvider::new(vec![text_response("I see you injected something")]);

    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Start".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    // Inject an additional message before the first turn
    iter.inject_message(Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Injected context".to_string())],
    });

    // Messages should include both the original and injected message
    assert_eq!(iter.messages().len(), 2);

    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::FinalResponse(agent_result) => {
            assert_eq!(agent_result.response, "I see you injected something");
        }
        other => panic!("expected FinalResponse, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_run_step_tools_mut() {
    // Start with no tools, add one between steps
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "dynamic"})),
        text_response("Done with dynamic tool"),
    ]);

    let tools = ToolRegistry::new(); // Empty initially
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Use echo".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    // Register tool dynamically before first turn
    iter.tools_mut().register(EchoTool);

    // First turn: tool execution should succeed now
    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::ToolsExecuted { calls, .. } => {
            assert_eq!(calls.len(), 1);
        }
        other => panic!("expected ToolsExecuted, got: {other:?}"),
    }

    // Second turn: final response
    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::FinalResponse(agent_result) => {
            assert_eq!(agent_result.response, "Done with dynamic tool");
        }
        other => panic!("expected FinalResponse, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_run_step_max_turns_reached() {
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "1"})),
        tool_use_response("call-2", "echo", serde_json::json!({"text": "2"})),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        max_turns: Some(1),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    // First turn succeeds
    let result = iter.next().await.expect("should have a turn");
    assert!(matches!(result, TurnResult::ToolsExecuted { .. }));

    // Second turn: max turns reached
    let result = iter.next().await.expect("should have a turn");
    assert!(matches!(result, TurnResult::MaxTurnsReached));

    // No more turns
    assert!(iter.next().await.is_none());
}

// --- Issue I-5 tests: LoopIteration hook event ---

#[tokio::test]
async fn test_loop_iteration_event_fired_in_run() {
    let provider = MockProvider::new(vec![text_response("Hello")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    let recorded = events.lock().expect("lock");
    assert!(
        recorded.contains(&"LoopIteration".to_string()),
        "expected LoopIteration event, got: {recorded:?}"
    );
}

#[tokio::test]
async fn test_loop_iteration_event_fired_in_run_step() {
    let provider = MockProvider::new(vec![text_response("Hello")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);
    let _ = iter.next().await;

    let recorded = events.lock().expect("lock");
    assert!(
        recorded.contains(&"LoopIteration".to_string()),
        "expected LoopIteration event in run_step, got: {recorded:?}"
    );
}

// --- Issue I-8 tests: run_stream() hooks and durability ---

/// A mock provider that supports streaming by returning StreamHandles.
/// Each call to complete_stream() produces a stream of events ending in MessageComplete.
struct MockStreamProvider {
    /// Pre-configured responses, returned as streams.
    responses: Mutex<Vec<CompletionResponse>>,
}

impl MockStreamProvider {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl neuron_types::Provider for MockStreamProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        Err(ProviderError::InvalidRequest("use complete_stream".into()))
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        let response = {
            let mut responses = self.responses.lock().expect("test lock poisoned");
            if responses.is_empty() {
                panic!("MockStreamProvider: no more responses configured");
            }
            responses.remove(0)
        };

        // Build stream events from the response: text deltas + MessageComplete
        let mut events: Vec<StreamEvent> = Vec::new();
        for block in &response.message.content {
            match block {
                ContentBlock::Text(text) => {
                    events.push(StreamEvent::TextDelta(text.clone()));
                }
                ContentBlock::ToolUse { id, name, .. } => {
                    events.push(StreamEvent::ToolUseStart {
                        id: id.clone(),
                        name: name.clone(),
                    });
                    events.push(StreamEvent::ToolUseEnd { id: id.clone() });
                }
                _ => {}
            }
        }
        events.push(StreamEvent::Usage(response.usage.clone()));
        events.push(StreamEvent::MessageComplete(response.message.clone()));

        let event_stream = stream::iter(events);
        Ok(StreamHandle {
            receiver: Box::pin(event_stream),
        })
    }
}

#[tokio::test]
async fn test_run_stream_fires_pre_and_post_llm_hooks() {
    let provider = MockStreamProvider::new(vec![text_response("Streamed hello")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    // Drain the stream
    while let Some(_event) = rx.recv().await {}

    let recorded = events.lock().expect("lock");
    assert!(
        recorded.contains(&"PreLlmCall".to_string()),
        "expected PreLlmCall event in run_stream, got: {recorded:?}"
    );
    assert!(
        recorded.contains(&"PostLlmCall".to_string()),
        "expected PostLlmCall event in run_stream, got: {recorded:?}"
    );
    assert!(
        recorded.contains(&"LoopIteration".to_string()),
        "expected LoopIteration event in run_stream, got: {recorded:?}"
    );
}

#[tokio::test]
async fn test_run_stream_fires_tool_hooks() {
    let provider = MockStreamProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("Done streaming"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;
    while let Some(_event) = rx.recv().await {}

    let recorded = events.lock().expect("lock");
    assert!(
        recorded.contains(&"PreToolExecution:echo".to_string()),
        "expected PreToolExecution:echo event in run_stream, got: {recorded:?}"
    );
    assert!(
        recorded.contains(&"PostToolExecution:echo".to_string()),
        "expected PostToolExecution:echo event in run_stream, got: {recorded:?}"
    );
}

#[tokio::test]
async fn test_run_stream_routes_through_durable_context() {
    let provider = MockStreamProvider::new(vec![]); // empty, should not be called

    let (durable, llm_calls, _tool_calls) =
        MockDurable::new(vec![text_response("Durable streamed")]);

    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    agent.set_durability(durable);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;
    while let Some(_event) = rx.recv().await {}

    let llm = llm_calls.lock().expect("lock");
    assert_eq!(llm.len(), 1, "expected 1 durable LLM call, got: {llm:?}");
}

// --- Additional test coverage: hook actions in run_stream ---

/// A hook that terminates the loop on PreLlmCall during streaming.
struct StreamTerminatingHook;

impl ObservabilityHook for StreamTerminatingHook {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
        let action = match event {
            HookEvent::PreLlmCall { .. } => HookAction::Terminate {
                reason: "stream terminated by hook".to_string(),
            },
            _ => HookAction::Continue,
        };
        std::future::ready(Ok(action))
    }
}

#[tokio::test]
async fn test_run_stream_terminate_stops_streaming() {
    let provider = MockStreamProvider::new(vec![text_response("Should not reach")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    agent.add_hook(StreamTerminatingHook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    // Collect all events
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Should have received an error event about termination
    let has_termination = events.iter().any(|e| {
        matches!(e, StreamEvent::Error(err) if err.message.contains("stream terminated by hook"))
    });
    assert!(
        has_termination,
        "expected termination error event, got: {events:?}"
    );

    // Should NOT have any MessageComplete since we terminated before the LLM call
    let has_message_complete = events
        .iter()
        .any(|e| matches!(e, StreamEvent::MessageComplete(_)));
    assert!(
        !has_message_complete,
        "should not have MessageComplete after termination"
    );
}

/// A hook that skips tool execution during streaming.
struct StreamSkipToolHook;

impl ObservabilityHook for StreamSkipToolHook {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
        let action = match event {
            HookEvent::PreToolExecution { .. } => HookAction::Skip {
                reason: "stream tool blocked".to_string(),
            },
            _ => HookAction::Continue,
        };
        std::future::ready(Ok(action))
    }
}

#[tokio::test]
async fn test_run_stream_skip_tool_sends_rejection() {
    let provider = MockStreamProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("OK, tool was skipped"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    agent.add_hook(StreamSkipToolHook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    // Collect all events
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Should have a MessageComplete with the final text response
    let has_final_response = events.iter().any(|e| {
        if let StreamEvent::MessageComplete(msg) = e {
            msg.content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text(t) if t == "OK, tool was skipped"))
        } else {
            false
        }
    });
    assert!(
        has_final_response,
        "expected final response after skip, got: {events:?}"
    );

    // Verify the loop continued after skip (the tool result message in the conversation
    // should contain the skip reason)
    let messages = agent.messages();
    let has_skip_result = messages.iter().any(|m| {
        m.content.iter().any(|b| {
            if let ContentBlock::ToolResult {
                content, is_error, ..
            } = b
            {
                *is_error
                    && content.iter().any(|c| {
                        if let ContentItem::Text(t) = c {
                            t.contains("stream tool blocked")
                        } else {
                            false
                        }
                    })
            } else {
                false
            }
        })
    });
    assert!(
        has_skip_result,
        "expected tool result with skip reason in messages"
    );
}

#[tokio::test]
async fn test_loop_iteration_fires_every_turn_multi_turn() {
    // Provider returns tool call then final text (2 turns)
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "first"})),
        tool_use_response("call-2", "echo", serde_json::json!({"text": "second"})),
        text_response("Final after two tool turns"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    /// A hook that records LoopIteration turn numbers.
    struct TurnTracker {
        turns: Arc<Mutex<Vec<usize>>>,
    }

    impl ObservabilityHook for TurnTracker {
        fn on_event(
            &self,
            event: HookEvent<'_>,
        ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
            if let HookEvent::LoopIteration { turn } = event {
                self.turns.lock().expect("lock").push(turn);
            }
            std::future::ready(Ok(HookAction::Continue))
        }
    }

    let turns_log = Arc::new(Mutex::new(Vec::new()));
    let hook = TurnTracker {
        turns: turns_log.clone(),
    };
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go multi-turn".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");
    assert_eq!(result.turns, 3);

    let turns = turns_log.lock().expect("lock");
    assert_eq!(
        *turns,
        vec![0, 1, 2],
        "expected LoopIteration for turns 0, 1, 2, got: {turns:?}"
    );
}

#[tokio::test]
async fn test_context_compaction_fires_during_streaming() {
    // Use a very low max_tokens so compaction triggers.
    let provider = MockStreamProvider::new(vec![
        tool_use_response(
            "call-1",
            "echo",
            serde_json::json!({"text": "first message with enough words to generate tokens"}),
        ),
        tool_use_response(
            "call-2",
            "echo",
            serde_json::json!({"text": "second message with even more words for token counting"}),
        ),
        text_response("Final after compaction in stream"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    // Very low max_tokens to force compaction, window_size of 2
    let context = SlidingWindowStrategy::new(2, 50);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text(
            "Start with a reasonably long message so tokens accumulate quickly for testing"
                .to_string(),
        )],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;
    while let Some(_event) = rx.recv().await {}

    let recorded = events.lock().expect("lock");
    assert!(
        recorded.contains(&"ContextCompaction".to_string()),
        "expected ContextCompaction event during streaming, got: {recorded:?}"
    );
}

// --- Batch 1 tests: CancellationToken + Parallel Tool Execution ---

/// Helper to create a CompletionResponse with multiple tool calls.
fn multi_tool_use_response(tools: &[(&str, &str, serde_json::Value)]) -> CompletionResponse {
    let content = tools
        .iter()
        .map(|(id, name, input)| ContentBlock::ToolUse {
            id: id.to_string(),
            name: name.to_string(),
            input: input.clone(),
        })
        .collect();
    CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content,
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason: StopReason::ToolUse,
    }
}

#[tokio::test]
async fn test_cancellation_stops_loop() {
    let provider = MockProvider::new(vec![text_response("Should not reach this")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    // Create a pre-cancelled token
    let token = CancellationToken::new();
    token.cancel();

    let tool_ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test-session".to_string(),
        environment: HashMap::new(),
        cancellation_token: token,
        progress_reporter: None,
    };

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    let err = agent.run(user_msg, &tool_ctx).await.unwrap_err();
    assert!(
        matches!(err, LoopError::Cancelled),
        "expected Cancelled, got: {err:?}"
    );
}

#[tokio::test]
async fn test_cancellation_during_tool_execution() {
    // Provider returns a tool call, but the token is already cancelled
    // so the check before the tool for-loop catches it.
    let provider = MockProvider::new(vec![tool_use_response(
        "call-1",
        "echo",
        serde_json::json!({"text": "hello"}),
    )]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    // Use a hook that cancels the token after the LLM call completes
    // but before tool execution
    let token = CancellationToken::new();
    let token_clone = token.clone();

    struct CancelOnPostLlmHook {
        token: CancellationToken,
    }

    impl ObservabilityHook for CancelOnPostLlmHook {
        fn on_event(
            &self,
            event: HookEvent<'_>,
        ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
            if matches!(event, HookEvent::PostLlmCall { .. }) {
                self.token.cancel();
            }
            std::future::ready(Ok(HookAction::Continue))
        }
    }

    agent.add_hook(CancelOnPostLlmHook { token: token_clone });

    let tool_ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test-session".to_string(),
        environment: HashMap::new(),
        cancellation_token: token,
        progress_reporter: None,
    };

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo hello".to_string())],
    };

    let err = agent.run(user_msg, &tool_ctx).await.unwrap_err();
    assert!(
        matches!(err, LoopError::Cancelled),
        "expected Cancelled, got: {err:?}"
    );
}

#[tokio::test]
async fn test_parallel_tool_execution_all_results() {
    // Provider returns 2 tool calls in one response, then a final text.
    let provider = MockProvider::new(vec![
        multi_tool_use_response(&[
            ("call-1", "echo", serde_json::json!({"text": "alpha"})),
            ("call-2", "echo", serde_json::json!({"text": "beta"})),
        ]),
        text_response("Both tools executed"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        parallel_tool_execution: true,
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Run both".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    assert_eq!(result.response, "Both tools executed");
    assert_eq!(result.turns, 2);

    // Verify both tool results are in the messages
    let tool_result_msg = result
        .messages
        .iter()
        .find(|m| {
            m.role == Role::User
                && m.content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
        })
        .expect("should have a tool result message");

    let tool_result_count = tool_result_msg
        .content
        .iter()
        .filter(|b| matches!(b, ContentBlock::ToolResult { .. }))
        .count();
    assert_eq!(tool_result_count, 2, "expected 2 tool results");
}

#[tokio::test]
async fn test_sequential_tool_execution_order() {
    // Verify that when parallel_tool_execution is false (default),
    // tools are executed in the order they appear in the response.
    // We use a recording hook to capture PreToolExecution events in order.
    let provider = MockProvider::new(vec![
        multi_tool_use_response(&[
            ("call-1", "echo", serde_json::json!({"text": "first"})),
            ("call-2", "echo", serde_json::json!({"text": "second"})),
            ("call-3", "echo", serde_json::json!({"text": "third"})),
        ]),
        text_response("All done"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        parallel_tool_execution: false,
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);

    // Use a hook that records the input value from each PreToolExecution event.
    let call_order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let call_order_clone = call_order.clone();

    struct OrderTracker {
        order: Arc<Mutex<Vec<String>>>,
    }

    impl ObservabilityHook for OrderTracker {
        fn on_event(
            &self,
            event: HookEvent<'_>,
        ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
            if let HookEvent::PreToolExecution { input, .. } = &event
                && let Some(text) = input.get("text").and_then(|v| v.as_str())
            {
                self.order.lock().expect("lock").push(text.to_string());
            }
            std::future::ready(Ok(HookAction::Continue))
        }
    }

    agent.add_hook(OrderTracker {
        order: call_order_clone,
    });

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Run all".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    assert_eq!(result.response, "All done");

    let log = call_order.lock().expect("lock");
    assert_eq!(
        *log,
        vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string()
        ],
        "tools should execute in order when parallel_tool_execution is false"
    );
}

// ==========================================================================
// Priority 1 — run() error branches
// ==========================================================================

// --- Test 1: Server-side compaction (StopReason::Compaction) ---

/// Helper to create a CompletionResponse with a specific stop_reason.
fn response_with_stop_reason(text: &str, stop_reason: StopReason) -> CompletionResponse {
    CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text(text.to_string())],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason,
    }
}

#[tokio::test]
async fn test_server_side_compaction_continues_loop() {
    // First turn: provider returns StopReason::Compaction (server paused to compact)
    // Second turn: provider returns EndTurn with the final response
    let provider = MockProvider::new(vec![
        response_with_stop_reason("compacting...", StopReason::Compaction),
        text_response("Final after server compaction"),
    ]);

    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed after server-side compaction");

    assert_eq!(result.response, "Final after server compaction");
    // Two turns: the compaction turn + the final turn
    assert_eq!(result.turns, 2);
    // Usage accumulated from both turns: 10+10 input, 5+5 output
    assert_eq!(result.usage.input_tokens, 20);
    assert_eq!(result.usage.output_tokens, 10);
}

// --- Test 2: ToolError::ModelRetry in streaming path ---

/// A tool that always returns ToolError::ModelRetry.
/// Implemented as ToolDyn directly since Tool::Error doesn't map to ModelRetry.
struct ModelRetryTool;

impl ToolDyn for ModelRetryTool {
    fn name(&self) -> &str {
        "retry_tool"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "retry_tool".to_string(),
            title: Some("Retry Tool".to_string()),
            description: "A tool that always requests a model retry".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    fn call_dyn<'a>(
        &'a self,
        _input: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            Err(ToolError::ModelRetry(
                "wrong arguments, try using field 'query'".to_string(),
            ))
        })
    }
}

#[tokio::test]
async fn test_model_retry_converts_to_error_tool_result_in_stream() {
    // The streaming path in run_stream() converts ModelRetry to an error
    // tool result so the model can self-correct.
    let provider = MockStreamProvider::new(vec![
        tool_use_response("call-1", "retry_tool", serde_json::json!({})),
        text_response("I corrected my approach"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register_dyn(Arc::new(ModelRetryTool));

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Do something".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    // Drain the stream
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Should have completed successfully (no StreamEvent::Error for ModelRetry)
    let has_final_response = events.iter().any(|e| {
        if let StreamEvent::MessageComplete(msg) = e {
            msg.content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text(t) if t == "I corrected my approach"))
        } else {
            false
        }
    });
    assert!(
        has_final_response,
        "expected final response after ModelRetry, got: {events:?}"
    );

    // Verify the tool result with the hint was appended to messages
    let messages = agent.messages();
    let has_retry_hint = messages.iter().any(|m| {
        m.content.iter().any(|b| {
            if let ContentBlock::ToolResult {
                content, is_error, ..
            } = b
            {
                *is_error
                    && content.iter().any(|c| {
                        if let ContentItem::Text(t) = c {
                            t.contains("wrong arguments, try using field 'query'")
                        } else {
                            false
                        }
                    })
            } else {
                false
            }
        })
    });
    assert!(
        has_retry_hint,
        "expected ModelRetry hint in tool result messages"
    );
}

// --- Test 3: accumulate_usage with optional fields ---

#[tokio::test]
async fn test_accumulate_usage_optional_fields_via_run() {
    // Two-turn conversation where each turn has different optional usage fields.
    let response1 = CompletionResponse {
        id: "r1".to_string(),
        model: "mock".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "call-1".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({"text": "hi"}),
            }],
        },
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: Some(10),
            cache_creation_tokens: None,
            reasoning_tokens: Some(20),
            ..Default::default()
        },
        stop_reason: StopReason::ToolUse,
    };

    let response2 = CompletionResponse {
        id: "r2".to_string(),
        model: "mock".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("Done".to_string())],
        },
        usage: TokenUsage {
            input_tokens: 200,
            output_tokens: 75,
            cache_read_tokens: Some(5),
            cache_creation_tokens: Some(8),
            reasoning_tokens: None,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    };

    let provider = MockProvider::new(vec![response1, response2]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    // Verify cumulative usage
    assert_eq!(result.usage.input_tokens, 300); // 100 + 200
    assert_eq!(result.usage.output_tokens, 125); // 50 + 75
    assert_eq!(result.usage.cache_read_tokens, Some(15)); // 10 + 5
    assert_eq!(result.usage.cache_creation_tokens, Some(8)); // None + 8
    assert_eq!(result.usage.reasoning_tokens, Some(20)); // 20 + None
}

#[tokio::test]
async fn test_accumulate_usage_all_none_stays_none() {
    // Single turn with no optional usage fields.
    let response = CompletionResponse {
        id: "r1".to_string(),
        model: "mock".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("Hello".to_string())],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: None,
            cache_creation_tokens: None,
            reasoning_tokens: None,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    };

    let provider = MockProvider::new(vec![response]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed");

    assert_eq!(result.usage.input_tokens, 10);
    assert_eq!(result.usage.output_tokens, 5);
    assert_eq!(result.usage.cache_read_tokens, None);
    assert_eq!(result.usage.cache_creation_tokens, None);
    assert_eq!(result.usage.reasoning_tokens, None);
}

// ==========================================================================
// Priority 2 — run_step() error branches
// ==========================================================================

// --- Test 4: Compaction error in run_step ---

/// A context strategy that always wants to compact but fails.
struct FailingCompactionContext;

impl ContextStrategy for FailingCompactionContext {
    fn should_compact(&self, _messages: &[Message], _token_count: usize) -> bool {
        true
    }

    async fn compact(&self, _messages: Vec<Message>) -> Result<Vec<Message>, ContextError> {
        Err(ContextError::CompactionFailed(
            "test compaction failure".to_string(),
        ))
    }

    fn token_estimate(&self, _messages: &[Message]) -> usize {
        999_999 // always high to trigger compaction
    }
}

#[tokio::test]
async fn test_run_step_compaction_error_returns_turn_result_error() {
    let provider = MockProvider::new(vec![text_response("Should not reach")]);
    let tools = ToolRegistry::new();
    let context = FailingCompactionContext;
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::Error(LoopError::Context(ContextError::CompactionFailed(msg))) => {
            assert!(
                msg.contains("test compaction failure"),
                "expected compaction failure message, got: {msg}"
            );
        }
        other => panic!("expected TurnResult::Error(LoopError::Context), got: {other:?}"),
    }

    // Iterator should be finished
    assert!(iter.next().await.is_none());
}

// --- Test 5: Provider error in run_step ---

/// A mock provider that always returns an error.
struct ErrorProvider;

impl neuron_types::Provider for ErrorProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        Err(ProviderError::InvalidRequest(
            "test provider error".to_string(),
        ))
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest(
            "test provider error".to_string(),
        ))
    }
}

#[tokio::test]
async fn test_run_step_provider_error_returns_turn_result_error() {
    let provider = ErrorProvider;
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::Error(LoopError::Provider(ProviderError::InvalidRequest(msg))) => {
            assert!(
                msg.contains("test provider error"),
                "expected provider error message, got: {msg}"
            );
        }
        other => panic!("expected TurnResult::Error(LoopError::Provider), got: {other:?}"),
    }

    // Iterator should be finished
    assert!(iter.next().await.is_none());
}

// --- Test 6: Server-side compaction in run_step ---

#[tokio::test]
async fn test_run_step_server_side_compaction_yields_compaction_event() {
    // First turn: provider returns StopReason::Compaction
    // Second turn: provider returns EndTurn
    let provider = MockProvider::new(vec![
        response_with_stop_reason("compacting...", StopReason::Compaction),
        text_response("Final after compaction"),
    ]);

    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    // First turn: should be CompactionOccurred (server-side)
    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::CompactionOccurred {
            old_tokens,
            new_tokens,
        } => {
            // Server-side compaction reports 0/0 since the server handles it
            assert_eq!(old_tokens, 0);
            assert_eq!(new_tokens, 0);
        }
        other => panic!("expected CompactionOccurred, got: {other:?}"),
    }

    // Second turn: should be FinalResponse
    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::FinalResponse(agent_result) => {
            assert_eq!(agent_result.response, "Final after compaction");
        }
        other => panic!("expected FinalResponse, got: {other:?}"),
    }

    // No more turns
    assert!(iter.next().await.is_none());
}

// --- Test 7: Parallel tool error in run_step ---

/// A tool that always fails with ExecutionFailed.
struct FailingTool;

impl ToolDyn for FailingTool {
    fn name(&self) -> &str {
        "failing_tool"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "failing_tool".to_string(),
            title: Some("Failing Tool".to_string()),
            description: "A tool that always fails".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    fn call_dyn<'a>(
        &'a self,
        _input: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            Err(ToolError::ExecutionFailed(
                "intentional test failure".into(),
            ))
        })
    }
}

#[tokio::test]
async fn test_run_step_parallel_tool_error() {
    // Provider returns two tool calls: one echo (succeeds) and one failing_tool (fails).
    // With parallel execution, the error should propagate.
    let provider = MockProvider::new(vec![multi_tool_use_response(&[
        ("call-1", "echo", serde_json::json!({"text": "alpha"})),
        ("call-2", "failing_tool", serde_json::json!({})),
    ])]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);
    tools.register_dyn(Arc::new(FailingTool));

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        parallel_tool_execution: true,
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Run both".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::Error(LoopError::Tool(ToolError::ExecutionFailed(_))) => {
            // Expected: parallel tool execution propagates the error
        }
        other => panic!("expected TurnResult::Error(LoopError::Tool), got: {other:?}"),
    }

    // Iterator should be finished after error
    assert!(iter.next().await.is_none());
}

// ==========================================================================
// Priority 3 — Streaming error branches
// ==========================================================================

// --- Test 8: Streaming provider error ---

/// A mock streaming provider that returns an error from complete_stream.
struct ErrorStreamProvider;

impl neuron_types::Provider for ErrorStreamProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        Err(ProviderError::InvalidRequest("use stream".into()))
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::ServiceUnavailable(
            "test stream provider error".to_string(),
        ))
    }
}

#[tokio::test]
async fn test_run_stream_provider_error() {
    let provider = ErrorStreamProvider;
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    // Collect all events
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Should have received an error event about the provider failure
    let has_provider_error = events
        .iter()
        .any(|e| matches!(e, StreamEvent::Error(err) if err.message.contains("provider error")));
    assert!(
        has_provider_error,
        "expected provider error event in stream, got: {events:?}"
    );

    // Should NOT have any MessageComplete since the provider failed
    let has_message_complete = events
        .iter()
        .any(|e| matches!(e, StreamEvent::MessageComplete(_)));
    assert!(
        !has_message_complete,
        "should not have MessageComplete after provider error"
    );
}

// --- Test 9: ModelRetry during streaming (already covered above in test 2,
//     but let's also verify the hint text propagates correctly) ---

#[tokio::test]
async fn test_run_stream_model_retry_hint_is_sent_as_error_tool_result() {
    // A tool that returns ModelRetry, then the model self-corrects.
    // This specifically tests the streaming code path in step.rs.
    let provider = MockStreamProvider::new(vec![
        tool_use_response("call-1", "retry_tool", serde_json::json!({})),
        text_response("After self-correction"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register_dyn(Arc::new(ModelRetryTool));

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let (hook, hook_events) = RecordingHook::new();
    agent.add_hook(hook);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Use the tool".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Should NOT have any StreamEvent::Error for the ModelRetry
    let has_fatal_error = events
        .iter()
        .any(|e| matches!(e, StreamEvent::Error(err) if err.message.contains("tool error")));
    assert!(
        !has_fatal_error,
        "ModelRetry should not produce a fatal stream error, got: {events:?}"
    );

    // The final response should arrive
    let has_final = events.iter().any(|e| {
        if let StreamEvent::MessageComplete(msg) = e {
            msg.content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text(t) if t == "After self-correction"))
        } else {
            false
        }
    });
    assert!(has_final, "expected final response after ModelRetry");

    // Hooks should have fired (PreToolExecution + PostToolExecution for retry_tool)
    let recorded = hook_events.lock().expect("lock");
    assert!(
        recorded.contains(&"PreToolExecution:retry_tool".to_string()),
        "expected PreToolExecution hook for retry_tool"
    );
    // Note: PostToolExecution is NOT fired for ModelRetry since the error
    // is caught before reaching the post-hook path in run_stream.
}

// --- Test 10: Tool execution error during streaming ---

#[tokio::test]
async fn test_run_stream_tool_execution_error() {
    // A tool that fails with ExecutionFailed during streaming.
    // This should produce a StreamEvent::Error.
    let provider = MockStreamProvider::new(vec![tool_use_response(
        "call-1",
        "failing_tool",
        serde_json::json!({}),
    )]);

    let mut tools = ToolRegistry::new();
    tools.register_dyn(Arc::new(FailingTool));

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Do something".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Should have received an error event about the tool failure
    let has_tool_error = events
        .iter()
        .any(|e| matches!(e, StreamEvent::Error(err) if err.message.contains("tool error")));
    assert!(
        has_tool_error,
        "expected tool error event in stream, got: {events:?}"
    );
}

// ==========================================================================
// Additional edge case coverage
// ==========================================================================

#[tokio::test]
async fn test_run_provider_error_propagates() {
    // Provider returns an error during run()
    let provider = ErrorProvider;
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::Provider(ProviderError::InvalidRequest(msg)) => {
            assert!(msg.contains("test provider error"));
        }
        other => panic!("expected LoopError::Provider, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_run_compaction_error_propagates() {
    // Context strategy fails during compaction in run()
    let provider = MockProvider::new(vec![text_response("Should not reach")]);
    let tools = ToolRegistry::new();
    let context = FailingCompactionContext;
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::Context(ContextError::CompactionFailed(msg)) => {
            assert!(msg.contains("test compaction failure"));
        }
        other => panic!("expected LoopError::Context, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_server_side_compaction_multiple_consecutive() {
    // Provider returns Compaction twice, then EndTurn.
    // Tests that the loop correctly handles multiple consecutive compaction events.
    let provider = MockProvider::new(vec![
        response_with_stop_reason("first compaction", StopReason::Compaction),
        response_with_stop_reason("second compaction", StopReason::Compaction),
        text_response("Final after two compactions"),
    ]);

    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed after multiple compactions");

    assert_eq!(result.response, "Final after two compactions");
    assert_eq!(result.turns, 3); // 2 compaction turns + 1 final
}

#[tokio::test]
async fn test_run_stream_compaction_error_sends_error_event() {
    // Context strategy fails during compaction in run_stream()
    let provider = MockStreamProvider::new(vec![text_response("Should not reach")]);
    let tools = ToolRegistry::new();
    let context = FailingCompactionContext;
    let config = LoopConfig::default();

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    let has_compaction_error = events
        .iter()
        .any(|e| matches!(e, StreamEvent::Error(err) if err.message.contains("compaction error")));
    assert!(
        has_compaction_error,
        "expected compaction error event in stream, got: {events:?}"
    );
}

#[tokio::test]
async fn test_run_stream_max_turns_sends_error_event() {
    // Streaming with max_turns exceeded
    let provider = MockStreamProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "1"})),
        tool_use_response("call-2", "echo", serde_json::json!({"text": "2"})),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        max_turns: Some(1),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    let has_max_turns_error = events
        .iter()
        .any(|e| matches!(e, StreamEvent::Error(err) if err.message.contains("max turns reached")));
    assert!(
        has_max_turns_error,
        "expected max turns error event in stream, got: {events:?}"
    );
}

#[tokio::test]
async fn test_run_step_sequential_tool_error() {
    // Like test 7 but with sequential execution (parallel_tool_execution = false).
    // The failing tool should produce TurnResult::Error.
    let provider = MockProvider::new(vec![tool_use_response(
        "call-1",
        "failing_tool",
        serde_json::json!({}),
    )]);

    let mut tools = ToolRegistry::new();
    tools.register_dyn(Arc::new(FailingTool));

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        parallel_tool_execution: false,
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::Error(LoopError::Tool(ToolError::ExecutionFailed(_))) => {
            // Expected
        }
        other => panic!("expected TurnResult::Error(LoopError::Tool), got: {other:?}"),
    }

    assert!(iter.next().await.is_none());
}

// ==========================================================================
// UsageLimits enforcement tests
// ==========================================================================

#[tokio::test]
async fn test_usage_limits_request_limit_exceeded() {
    // Provider always returns tool calls. With request_limit = 1, the first
    // request succeeds (returning a tool call), but the second iteration
    // fails the pre-request limit check.
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "first"})),
        // Second response would be consumed if the limit didn't fire first.
        text_response("Should not reach this"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_request_limit(1)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Keep going".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::UsageLimitExceeded(msg) => {
            assert!(
                msg.contains("request limit"),
                "expected 'request limit' in message, got: {msg}"
            );
        }
        other => panic!("expected UsageLimitExceeded, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_usage_limits_token_limit_exceeded() {
    // Mock returns a response with high token usage that exceeds the total
    // tokens limit. The check happens post-response (after accumulate_usage).
    let high_usage_response = CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("big response".to_string())],
        },
        usage: TokenUsage {
            input_tokens: 500,
            output_tokens: 600,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    };

    let provider = MockProvider::new(vec![high_usage_response]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        // Total tokens limit of 100 — the response uses 500 + 600 = 1100
        usage_limits: Some(UsageLimits::default().with_total_tokens_limit(100)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::UsageLimitExceeded(msg) => {
            assert!(
                msg.contains("total token limit"),
                "expected 'total token limit' in message, got: {msg}"
            );
        }
        other => panic!("expected UsageLimitExceeded, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_usage_limits_tool_call_limit_exceeded() {
    // Provider returns a response with 3 tool calls, but the tool_calls_limit
    // is 2. The check happens before tool execution.
    let provider = MockProvider::new(vec![multi_tool_use_response(&[
        ("call-1", "echo", serde_json::json!({"text": "a"})),
        ("call-2", "echo", serde_json::json!({"text": "b"})),
        ("call-3", "echo", serde_json::json!({"text": "c"})),
    ])]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_tool_calls_limit(2)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Run all three".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::UsageLimitExceeded(msg) => {
            assert!(
                msg.contains("tool call limit"),
                "expected 'tool call limit' in message, got: {msg}"
            );
        }
        other => panic!("expected UsageLimitExceeded, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_usage_limits_none_allows_unlimited() {
    // No usage limits — the loop runs normally through tool calls and completes.
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("All done, no limits"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: None, // explicitly no limits
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed without usage limits");

    assert_eq!(result.response, "All done, no limits");
    assert_eq!(result.turns, 2);
    // Usage should be accumulated normally: 10+10 input, 5+5 output
    assert_eq!(result.usage.input_tokens, 20);
    assert_eq!(result.usage.output_tokens, 10);
}

// ==========================================================================
// Additional UsageLimits tests
// ==========================================================================

#[tokio::test]
async fn test_usage_limits_input_tokens_limit_exceeded() {
    // Mock returns a response whose input tokens exceed the configured limit.
    // Default text_response uses input_tokens=10, so set limit to 5.
    let provider = MockProvider::new(vec![text_response("big input")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_input_tokens_limit(5)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::UsageLimitExceeded(msg) => {
            assert!(
                msg.contains("input token limit"),
                "expected 'input token limit' in message, got: {msg}"
            );
        }
        other => panic!("expected UsageLimitExceeded for input tokens, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_usage_limits_output_tokens_limit_exceeded() {
    // Mock returns a response whose output tokens exceed the configured limit.
    // Default text_response uses output_tokens=5, so set limit to 3.
    let provider = MockProvider::new(vec![text_response("big output")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_output_tokens_limit(3)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::UsageLimitExceeded(msg) => {
            assert!(
                msg.contains("output token limit"),
                "expected 'output token limit' in message, got: {msg}"
            );
        }
        other => panic!("expected UsageLimitExceeded for output tokens, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_usage_limits_zero_request_limit() {
    // With request_limit=0, the very first iteration should fail the pre-request
    // check before any LLM call is made.
    let provider = MockProvider::new(vec![text_response("Should not reach")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_request_limit(0)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::UsageLimitExceeded(msg) => {
            assert!(
                msg.contains("request limit"),
                "expected 'request limit' in message, got: {msg}"
            );
        }
        other => panic!("expected UsageLimitExceeded for zero request limit, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_usage_limits_exact_boundary_passes() {
    // Set request_limit to 1. The mock returns EndTurn on the first call (no
    // tool calls). Should succeed because 1 request <= 1 limit (the check is
    // request_count >= max, and request_count is 0 when checked pre-request).
    let provider = MockProvider::new(vec![text_response("Exactly one request")]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_request_limit(1)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    };

    let result = agent
        .run(user_msg, &test_tool_context())
        .await
        .expect("run should succeed with exactly 1 request within limit of 1");

    assert_eq!(result.response, "Exactly one request");
    assert_eq!(result.turns, 1);
}

#[tokio::test]
async fn test_usage_limits_multiple_limits_first_hit_wins() {
    // Set request_limit=2 and total_tokens_limit=10. The first response uses
    // 10+5=15 total tokens, which exceeds the 10 total token limit. The token
    // limit should fire before the request limit is reached.
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "a"})),
        text_response("Should not reach"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(
            UsageLimits::default()
                .with_request_limit(2)
                .with_total_tokens_limit(10),
        ),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::UsageLimitExceeded(msg) => {
            assert!(
                msg.contains("total token limit"),
                "expected 'total token limit' in message (should fire before request limit), got: {msg}"
            );
        }
        other => panic!("expected UsageLimitExceeded for total tokens, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_usage_limits_step_iterator_enforced() {
    // Use run_step() instead of run(). Set request_limit=1.
    // First step should succeed (tool call). Second step should fail
    // because request_count (1) >= request_limit (1).
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hello"})),
        text_response("Should not reach"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_request_limit(1)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Go".to_string())],
    };
    let tool_ctx = test_tool_context();
    let mut iter = agent.run_step(user_msg, &tool_ctx);

    // First turn: tool call succeeds (request_count was 0 < 1)
    let result = iter.next().await.expect("should have a turn");
    assert!(
        matches!(result, TurnResult::ToolsExecuted { .. }),
        "expected ToolsExecuted, got: {result:?}"
    );

    // Second turn: request limit exceeded (request_count is now 1 >= 1)
    let result = iter.next().await.expect("should have a turn");
    match result {
        TurnResult::Error(LoopError::UsageLimitExceeded(msg)) => {
            assert!(
                msg.contains("request limit"),
                "expected 'request limit' in message, got: {msg}"
            );
        }
        other => panic!("expected TurnResult::Error(UsageLimitExceeded), got: {other:?}"),
    }

    // Iterator should be finished
    assert!(iter.next().await.is_none());
}

#[test]
fn test_usage_limits_builder_methods() {
    // Verify UsageLimits::default() has all fields as None.
    let defaults = UsageLimits::default();
    assert_eq!(defaults.request_limit, None);
    assert_eq!(defaults.tool_calls_limit, None);
    assert_eq!(defaults.input_tokens_limit, None);
    assert_eq!(defaults.output_tokens_limit, None);
    assert_eq!(defaults.total_tokens_limit, None);

    // Verify builder methods set the correct fields.
    let limits = UsageLimits::default()
        .with_request_limit(10)
        .with_tool_calls_limit(20)
        .with_input_tokens_limit(1000)
        .with_output_tokens_limit(500)
        .with_total_tokens_limit(1500);

    assert_eq!(limits.request_limit, Some(10));
    assert_eq!(limits.tool_calls_limit, Some(20));
    assert_eq!(limits.input_tokens_limit, Some(1000));
    assert_eq!(limits.output_tokens_limit, Some(500));
    assert_eq!(limits.total_tokens_limit, Some(1500));
}

#[tokio::test]
async fn test_usage_limits_tool_calls_cumulative_across_turns() {
    // First turn: 2 tool calls (within limit of 2).
    // Second turn: 1 more tool call. Cumulative count (2 + 1 = 3) exceeds limit of 2.
    let provider = MockProvider::new(vec![
        multi_tool_use_response(&[
            ("call-1", "echo", serde_json::json!({"text": "a"})),
            ("call-2", "echo", serde_json::json!({"text": "b"})),
        ]),
        tool_use_response("call-3", "echo", serde_json::json!({"text": "c"})),
        text_response("Should not reach"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_tool_calls_limit(2)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Run many tools".to_string())],
    };

    let err = agent.run(user_msg, &test_tool_context()).await.unwrap_err();
    match err {
        LoopError::UsageLimitExceeded(msg) => {
            assert!(
                msg.contains("tool call limit"),
                "expected 'tool call limit' in message, got: {msg}"
            );
        }
        other => panic!("expected UsageLimitExceeded for cumulative tool calls, got: {other:?}"),
    }
}

// ==========================================================================
// UsageLimits enforcement in run_stream() tests
// ==========================================================================

#[tokio::test]
async fn test_usage_limits_request_limit_in_stream() {
    // With request_limit=1, the first request succeeds (tool call), but the
    // second iteration triggers the request limit check.
    let provider = MockStreamProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "first"})),
        text_response("Should not reach this"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_request_limit(1)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Keep going".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Should end with an error event about the request limit
    let has_limit_error = events.iter().any(|e| {
        if let StreamEvent::Error(err) = e {
            err.message.contains("request limit")
        } else {
            false
        }
    });
    assert!(
        has_limit_error,
        "expected request limit error in stream events, got: {events:?}"
    );
}

#[tokio::test]
async fn test_usage_limits_token_limit_in_stream() {
    // The provider returns a response with high token usage exceeding the limit.
    let high_usage_response = CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("done".to_string())],
        },
        usage: TokenUsage {
            input_tokens: 50,
            output_tokens: 100,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    };

    let provider = MockStreamProvider::new(vec![high_usage_response]);
    let tools = ToolRegistry::new();
    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_total_tokens_limit(100)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Total usage is 150, limit is 100 — should trigger token limit error
    let has_token_error = events.iter().any(|e| {
        if let StreamEvent::Error(err) = e {
            err.message.contains("total token limit")
        } else {
            false
        }
    });
    assert!(
        has_token_error,
        "expected total token limit error in stream events, got: {events:?}"
    );
}

#[tokio::test]
async fn test_usage_limits_tool_call_limit_in_stream() {
    // Provider returns 3 tool calls but limit is 2.
    let response = CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::ToolUse {
                    id: "c1".to_string(),
                    name: "echo".to_string(),
                    input: serde_json::json!({"text": "a"}),
                },
                ContentBlock::ToolUse {
                    id: "c2".to_string(),
                    name: "echo".to_string(),
                    input: serde_json::json!({"text": "b"}),
                },
                ContentBlock::ToolUse {
                    id: "c3".to_string(),
                    name: "echo".to_string(),
                    input: serde_json::json!({"text": "c"}),
                },
            ],
        },
        usage: TokenUsage::default(),
        stop_reason: StopReason::ToolUse,
    };

    let provider = MockStreamProvider::new(vec![response]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        usage_limits: Some(UsageLimits::default().with_tool_calls_limit(2)),
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Run tools".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    let has_tool_limit_error = events.iter().any(|e| {
        if let StreamEvent::Error(err) = e {
            err.message.contains("tool call limit")
        } else {
            false
        }
    });
    assert!(
        has_tool_limit_error,
        "expected tool call limit error in stream events, got: {events:?}"
    );
}

// ==========================================================================
// Streaming tool execution parity tests
// ==========================================================================

#[tokio::test]
async fn test_run_stream_multi_tool_all_results() {
    // Provider returns 2 tool calls in one response, then a final text.
    // Streaming always executes tools sequentially, but all results must
    // be produced and fed back to the provider.
    let provider = MockStreamProvider::new(vec![
        multi_tool_use_response(&[
            ("call-1", "echo", serde_json::json!({"text": "alpha"})),
            ("call-2", "echo", serde_json::json!({"text": "beta"})),
        ]),
        text_response("Both tools executed"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    // parallel_tool_execution is irrelevant for streaming (always sequential)
    let config = LoopConfig {
        parallel_tool_execution: true,
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);
    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Run both".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // Should have text deltas from the final response
    let text_deltas: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let StreamEvent::TextDelta(text) = e {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(
        text_deltas.contains(&"Both tools executed"),
        "expected final text delta, got: {text_deltas:?}"
    );

    // Should NOT have any error events
    let errors: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, StreamEvent::Error(_)))
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}

#[tokio::test]
async fn test_run_stream_sequential_tool_order() {
    // Streaming always executes tools sequentially regardless of
    // parallel_tool_execution config. Verify execution order via hooks.
    let provider = MockStreamProvider::new(vec![
        multi_tool_use_response(&[
            ("call-1", "echo", serde_json::json!({"text": "first"})),
            ("call-2", "echo", serde_json::json!({"text": "second"})),
            ("call-3", "echo", serde_json::json!({"text": "third"})),
        ]),
        text_response("All done"),
    ]);

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);
    let config = LoopConfig {
        // Set to true to prove streaming ignores this and stays sequential
        parallel_tool_execution: true,
        ..LoopConfig::default()
    };

    let mut agent = AgentLoop::new(provider, tools, context, config);

    let call_order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let call_order_clone = call_order.clone();

    struct StreamOrderTracker {
        order: Arc<Mutex<Vec<String>>>,
    }

    impl ObservabilityHook for StreamOrderTracker {
        fn on_event(
            &self,
            event: HookEvent<'_>,
        ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
            if let HookEvent::PreToolExecution { input, .. } = &event
                && let Some(text) = input.get("text").and_then(|v| v.as_str())
            {
                self.order.lock().expect("lock").push(text.to_string());
            }
            std::future::ready(Ok(HookAction::Continue))
        }
    }

    agent.add_hook(StreamOrderTracker {
        order: call_order_clone,
    });

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Run all".to_string())],
    };

    let mut rx = agent.run_stream(user_msg, &test_tool_context()).await;
    while let Some(_event) = rx.recv().await {}

    let log = call_order.lock().expect("lock");
    assert_eq!(
        *log,
        vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string()
        ],
        "streaming should execute tools sequentially in order, got: {log:?}"
    );
}
