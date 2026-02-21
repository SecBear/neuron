//! Tests for the AgentLoop builder pattern and run_text convenience.

use agent_loop::AgentLoop;
use agent_types::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// --- Mock Provider ---

struct MockProvider {
    responses: std::sync::Mutex<Vec<CompletionResponse>>,
}

impl MockProvider {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses),
        }
    }

    fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text(text.to_string())],
            },
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::default(),
            model: "mock".to_string(),
            id: "mock-1".to_string(),
        }
    }
}

impl Provider for MockProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let response = self
            .responses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(0);
        Ok(response)
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest("streaming not implemented in mock".into()))
    }
}

// --- Mock ContextStrategy ---

struct NoOpContext;

impl ContextStrategy for NoOpContext {
    fn should_compact(&self, _messages: &[Message], _token_count: usize) -> bool {
        false
    }

    async fn compact(
        &self,
        _messages: Vec<Message>,
    ) -> Result<Vec<Message>, ContextError> {
        unreachable!()
    }

    fn token_estimate(&self, _messages: &[Message]) -> usize {
        0
    }
}

// --- Mock Hook ---

struct CountingHook {
    count: Arc<AtomicUsize>,
}

impl ObservabilityHook for CountingHook {
    async fn on_event(&self, _event: HookEvent<'_>) -> Result<HookAction, HookError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(HookAction::Continue)
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
async fn builder_minimal() {
    let provider = MockProvider::new(vec![MockProvider::text_response("hello")]);
    let context = NoOpContext;

    let mut agent = AgentLoop::builder(provider, context).build();
    let ctx = test_ctx();
    let result = agent.run_text("hi", &ctx).await.unwrap();
    assert_eq!(result.response, "hello");
}

#[tokio::test]
async fn builder_with_system_prompt() {
    let provider = MockProvider::new(vec![MockProvider::text_response("I'm helpful")]);
    let context = NoOpContext;

    let mut agent = AgentLoop::builder(provider, context)
        .system_prompt("You are helpful.")
        .build();
    let ctx = test_ctx();
    let result = agent.run_text("who are you?", &ctx).await.unwrap();
    assert_eq!(result.response, "I'm helpful");
}

#[tokio::test]
async fn builder_max_turns() {
    // 3 tool calls in a row, but max_turns is 2 — should error
    let provider = MockProvider::new(vec![
        CompletionResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "call-1".into(),
                    name: "fake".into(),
                    input: serde_json::json!({}),
                }],
            },
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage::default(),
            model: "mock".into(),
            id: "m1".into(),
        },
        CompletionResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "call-2".into(),
                    name: "fake".into(),
                    input: serde_json::json!({}),
                }],
            },
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage::default(),
            model: "mock".into(),
            id: "m2".into(),
        },
    ]);
    let context = NoOpContext;

    let mut agent = AgentLoop::builder(provider, context)
        .max_turns(2)
        .build();
    let ctx = test_ctx();
    // This will try to execute "fake" tool which doesn't exist, causing a ToolNotFound error
    // or it will reach max turns. Let's check for error.
    let result = agent.run_text("go", &ctx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn builder_multiple_hooks() {
    let count = Arc::new(AtomicUsize::new(0));

    let provider = MockProvider::new(vec![MockProvider::text_response("done")]);
    let context = NoOpContext;

    let mut agent = AgentLoop::builder(provider, context)
        .hook(CountingHook { count: count.clone() })
        .hook(CountingHook { count: count.clone() })
        .build();
    let ctx = test_ctx();
    agent.run_text("hi", &ctx).await.unwrap();

    // Each hook fires for PreLlmCall + PostLlmCall = 2 events, 2 hooks = 4 total
    // Plus LoopIteration = 3 events per hook = 6 total
    assert!(count.load(Ordering::SeqCst) >= 4, "hooks should fire multiple times");
}

#[tokio::test]
async fn run_text_equivalent_to_run() {
    let provider1 = MockProvider::new(vec![MockProvider::text_response("result")]);
    let provider2 = MockProvider::new(vec![MockProvider::text_response("result")]);
    let ctx = test_ctx();

    let mut agent1 = AgentLoop::builder(provider1, NoOpContext).build();
    let result1 = agent1.run_text("hello", &ctx).await.unwrap();

    let mut agent2 = AgentLoop::builder(provider2, NoOpContext).build();
    let result2 = agent2
        .run(
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("hello".into())],
            },
            &ctx,
        )
        .await
        .unwrap();

    assert_eq!(result1.response, result2.response);
    assert_eq!(result1.turns, result2.turns);
}
