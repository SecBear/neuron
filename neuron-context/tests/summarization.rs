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

// ---- Additional coverage tests ----

/// A provider that always returns an error.
#[derive(Clone)]
struct FailingProvider;

impl Provider for FailingProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        async {
            Err(ProviderError::InvalidRequest(
                "Internal server error".to_string(),
            ))
        }
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        async { Err(ProviderError::InvalidRequest("not supported".to_string())) }
    }
}

#[tokio::test]
async fn provider_error_propagates() {
    let strategy = SummarizationStrategy::new(FailingProvider, 2, 100_000);
    let messages = vec![
        user_msg("old1"),
        assistant_msg("old2"),
        user_msg("old3"),
        assistant_msg("recent1"),
        user_msg("recent2"),
    ];
    let result = strategy.compact(messages).await;
    assert!(result.is_err(), "provider error should propagate");
}

#[tokio::test]
async fn preserve_recent_larger_than_total_produces_summary_of_nothing() {
    // preserve_recent=100, but only 3 non-system messages → all are "recent"
    // old_messages is empty, so provider gets 0 messages to summarize
    let provider = MockProvider::new("Empty summary");
    let strategy = SummarizationStrategy::new(provider.clone(), 100, 100_000);
    let messages = vec![user_msg("m1"), assistant_msg("m2"), user_msg("m3")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // Provider was still called (with empty old_messages)
    assert_eq!(provider.call_count(), 1);
    // Result: 1 summary + 3 recent = 4
    assert_eq!(result.len(), 4);
}

#[tokio::test]
async fn multiple_system_messages_all_preserved() {
    let provider = MockProvider::new("Summary text");
    let strategy = SummarizationStrategy::new(provider, 1, 100_000);
    let messages = vec![
        system_msg("System rule 1"),
        system_msg("System rule 2"),
        system_msg("System rule 3"),
        user_msg("old message"),
        assistant_msg("old reply"),
        user_msg("recent"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // 3 system + 1 summary + 1 recent = 5
    assert_eq!(result.len(), 5);
    assert_eq!(result.iter().filter(|m| m.role == Role::System).count(), 3);
}

#[tokio::test]
async fn summary_message_has_correct_prefix() {
    let provider = MockProvider::new("The user discussed Rust.");
    let strategy = SummarizationStrategy::new(provider, 0, 100_000);
    let messages = vec![user_msg("Let's talk about Rust")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    assert_eq!(result.len(), 1);
    if let ContentBlock::Text(text) = &result[0].content[0] {
        assert!(
            text.starts_with("[Summary of earlier conversation]"),
            "summary should have correct prefix, got: {text}"
        );
        assert!(text.contains("The user discussed Rust."));
    } else {
        panic!("expected Text block");
    }
}

#[tokio::test]
async fn summary_message_role_is_user() {
    let provider = MockProvider::new("A summary");
    let strategy = SummarizationStrategy::new(provider, 0, 100_000);
    let messages = vec![user_msg("msg1")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    assert_eq!(result[0].role, Role::User);
}

#[test]
fn should_compact_exact_threshold_returns_false() {
    let provider = MockProvider::new("summary");
    let strategy = SummarizationStrategy::new(provider, 3, 5000);
    let msgs = vec![user_msg("hi")];
    // At exactly threshold, should not compact (> not >=)
    assert!(!strategy.should_compact(&msgs, 5000));
}

#[test]
fn token_estimate_delegates_to_counter() {
    let provider = MockProvider::new("summary");
    let strategy = SummarizationStrategy::new(provider, 3, 5000);
    let messages = vec![user_msg("hello world")];
    assert!(strategy.token_estimate(&messages) > 0);
}

#[test]
fn token_estimate_empty_messages() {
    let provider = MockProvider::new("summary");
    let strategy = SummarizationStrategy::new(provider, 3, 5000);
    assert_eq!(strategy.token_estimate(&[]), 0);
}

#[test]
fn with_counter_uses_custom_ratio() {
    let provider = MockProvider::new("summary");
    let counter = neuron_context::TokenCounter::with_ratio(2.0);
    let strategy = SummarizationStrategy::with_counter(provider, 3, 5000, counter);
    let messages = vec![user_msg("a".repeat(20).as_str())];
    let estimate = strategy.token_estimate(&messages);
    // 4 (overhead) + ceil(20/2) = 14
    assert_eq!(estimate, 14);
}

#[tokio::test]
async fn only_system_messages_still_calls_provider() {
    let provider = MockProvider::new("No non-system messages to summarize");
    let strategy = SummarizationStrategy::new(provider.clone(), 0, 100_000);
    let messages = vec![system_msg("rule 1"), system_msg("rule 2")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // Provider called (summarizing zero non-system messages)
    assert_eq!(provider.call_count(), 1);
    // 2 system + 1 summary = 3
    assert_eq!(result.len(), 3);
}

/// A provider that returns multiple text blocks in the response.
#[derive(Clone)]
struct MultiBlockProvider;

impl Provider for MultiBlockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        async {
            Ok(CompletionResponse {
                id: "mock-id".to_string(),
                model: "mock-model".to_string(),
                message: Message {
                    role: Role::Assistant,
                    content: vec![
                        ContentBlock::Text("Part one.".to_string()),
                        ContentBlock::Text("Part two.".to_string()),
                    ],
                },
                usage: TokenUsage {
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
        async { Err(ProviderError::InvalidRequest("not supported".to_string())) }
    }
}

#[tokio::test]
async fn multi_block_response_joined_with_newline() {
    let strategy = SummarizationStrategy::new(MultiBlockProvider, 0, 100_000);
    let messages = vec![user_msg("old message")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    if let ContentBlock::Text(text) = &result[0].content[0] {
        assert!(text.contains("Part one.\nPart two."));
    } else {
        panic!("expected Text block");
    }
}

/// A provider that returns a non-text block in the response (should be filtered out).
#[derive(Clone)]
struct NonTextBlockProvider;

impl Provider for NonTextBlockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        async {
            Ok(CompletionResponse {
                id: "mock-id".to_string(),
                model: "mock-model".to_string(),
                message: Message {
                    role: Role::Assistant,
                    content: vec![
                        ContentBlock::Text("Actual summary.".to_string()),
                        ContentBlock::ToolUse {
                            id: "tc1".to_string(),
                            name: "search".to_string(),
                            input: serde_json::json!({}),
                        },
                    ],
                },
                usage: TokenUsage {
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
        async { Err(ProviderError::InvalidRequest("not supported".to_string())) }
    }
}

#[tokio::test]
async fn non_text_blocks_in_response_are_filtered() {
    let strategy = SummarizationStrategy::new(NonTextBlockProvider, 0, 100_000);
    let messages = vec![user_msg("msg")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    if let ContentBlock::Text(text) = &result[0].content[0] {
        assert!(text.contains("Actual summary."));
        // ToolUse block should not appear in the summary text
        assert!(!text.contains("search"));
    } else {
        panic!("expected Text block");
    }
}
