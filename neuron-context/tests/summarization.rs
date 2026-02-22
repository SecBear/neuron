//! Integration tests for SummarizationStrategy.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use neuron_context::SummarizationStrategy;
use neuron_types::{
    CompletionRequest, CompletionResponse, ContentBlock, ContextStrategy, Message, Provider,
    ProviderError, Role, StopReason, StreamHandle, TokenUsage,
};

// ---- MockProvider -----------------------------------------------------------

/// A provider that always returns a fixed summary string.
#[derive(Clone)]
struct MockProvider {
    summary: String,
    call_count: Arc<AtomicUsize>,
}

impl MockProvider {
    fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl Provider for MockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let summary = self.summary.clone();
        async move {
            Ok(CompletionResponse {
                id: "mock-id".to_string(),
                model: "mock-model".to_string(),
                message: Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text(summary)],
                },
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    ..Default::default()
                },
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        async {
            Err(ProviderError::InvalidRequest(
                "streaming not supported in mock".to_string(),
            ))
        }
    }
}

// ---- Helpers ----------------------------------------------------------------

fn user_msg(text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

fn assistant_msg(text: &str) -> Message {
    Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

fn system_msg(text: &str) -> Message {
    Message {
        role: Role::System,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

// ---- Tests ------------------------------------------------------------------

#[tokio::test]
async fn summarizes_old_messages_preserves_recent() {
    let fixed_summary = "Summary of old messages";
    let provider = MockProvider::new(fixed_summary);
    let strategy = SummarizationStrategy::new(provider.clone(), 3, 100_000);

    // 10 non-system messages
    let messages: Vec<Message> = (0..10)
        .map(|i| {
            if i % 2 == 0 {
                user_msg(&format!("user message {i}"))
            } else {
                assistant_msg(&format!("assistant reply {i}"))
            }
        })
        .collect();

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // provider was called once
    assert_eq!(provider.call_count(), 1);

    // Result: 1 summary message + 3 recent messages
    assert_eq!(result.len(), 4);

    // First is the summary User message
    assert!(matches!(&result[0].role, Role::User));
    if let ContentBlock::Text(text) = &result[0].content[0] {
        assert!(
            text.contains(fixed_summary),
            "summary text should appear in result, got: {text}"
        );
    } else {
        panic!("expected Text block");
    }

    // Last 3 messages are the preserved recent ones (messages 7, 8, 9)
    if let ContentBlock::Text(t) = &result[1].content[0] {
        assert!(t.contains("7"), "expected message 7, got: {t}");
    }
}

#[tokio::test]
async fn system_messages_preserved_alongside_summary() {
    let provider = MockProvider::new("Compact summary");
    let strategy = SummarizationStrategy::new(provider, 2, 100_000);

    let messages = vec![
        system_msg("You are helpful."),
        user_msg("old message 1"),
        assistant_msg("old reply 1"),
        user_msg("old message 2"),
        assistant_msg("old reply 2"),
        user_msg("recent 1"),
        assistant_msg("recent 2"),
    ];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // System + summary + 2 recent = 4
    assert_eq!(result.len(), 4);
    assert_eq!(result.iter().filter(|m| m.role == Role::System).count(), 1);
}

#[tokio::test]
async fn preserve_recent_zero_summarizes_all() {
    let provider = MockProvider::new("Everything summarized");
    let strategy = SummarizationStrategy::new(provider.clone(), 0, 100_000);

    let messages = vec![user_msg("msg1"), assistant_msg("msg2"), user_msg("msg3")];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // Just the summary message
    assert_eq!(result.len(), 1);
    assert_eq!(provider.call_count(), 1);
}

#[test]
fn should_compact_respects_threshold() {
    let provider = MockProvider::new("summary");
    let strategy = SummarizationStrategy::new(provider, 3, 5000);
    let msgs = vec![user_msg("hi")];
    assert!(!strategy.should_compact(&msgs, 4999));
    assert!(strategy.should_compact(&msgs, 5001));
}
