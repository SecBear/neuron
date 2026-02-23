//! Integration tests for SlidingWindowStrategy.

use neuron_context::SlidingWindowStrategy;
use neuron_types::{ContentBlock, ContextStrategy, Message, Role};

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

#[tokio::test]
async fn keeps_only_last_n_non_system_messages() {
    let strategy = SlidingWindowStrategy::new(5, 100_000);

    // 10 non-system messages
    let mut messages: Vec<Message> = (0..10).map(|i| user_msg(&format!("message {i}"))).collect();
    messages.push(assistant_msg("final reply"));
    // total = 11 non-system

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // Should keep only the last 5
    assert_eq!(result.len(), 5);
    // The last message should be retained
    assert!(
        matches!(&result.last().unwrap().content[0], ContentBlock::Text(t) if t == "final reply")
    );
}

#[tokio::test]
async fn system_messages_are_always_preserved() {
    let strategy = SlidingWindowStrategy::new(3, 100_000);

    let messages = vec![
        system_msg("You are a helpful assistant."),
        system_msg("Second system rule."),
        user_msg("msg1"),
        assistant_msg("msg2"),
        user_msg("msg3"),
        assistant_msg("msg4"),
        user_msg("msg5"),
    ];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // 2 system + last 3 non-system = 5
    assert_eq!(result.len(), 5);
    assert_eq!(result.iter().filter(|m| m.role == Role::System).count(), 2);
    assert_eq!(result.iter().filter(|m| m.role != Role::System).count(), 3);
}

#[test]
fn should_compact_returns_false_under_threshold() {
    let strategy = SlidingWindowStrategy::new(5, 1000);
    let messages = vec![user_msg("short")];
    assert!(!strategy.should_compact(&messages, 500));
}

#[test]
fn should_compact_returns_true_over_threshold() {
    let strategy = SlidingWindowStrategy::new(5, 1000);
    let messages = vec![user_msg("short")];
    assert!(strategy.should_compact(&messages, 1001));
}

#[tokio::test]
async fn compact_fewer_messages_than_window_preserves_all() {
    let strategy = SlidingWindowStrategy::new(10, 100_000);

    let messages = vec![user_msg("only message"), assistant_msg("reply")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    assert_eq!(result.len(), 2);
}

#[test]
fn token_estimate_delegates_to_counter() {
    let strategy = SlidingWindowStrategy::new(5, 1000);
    let messages = vec![user_msg("hello world")];
    // Non-zero estimate
    assert!(strategy.token_estimate(&messages) > 0);
}

// ---- Additional coverage tests ----

#[tokio::test]
async fn compact_empty_messages_returns_empty() {
    let strategy = SlidingWindowStrategy::new(5, 100_000);
    let result = strategy
        .compact(vec![])
        .await
        .expect("compact should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn compact_only_system_messages_preserves_all() {
    let strategy = SlidingWindowStrategy::new(3, 100_000);
    let messages = vec![
        system_msg("System rule 1"),
        system_msg("System rule 2"),
        system_msg("System rule 3"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // All system messages preserved, no non-system messages to trim
    assert_eq!(result.len(), 3);
    assert!(result.iter().all(|m| m.role == Role::System));
}

#[tokio::test]
async fn compact_window_size_zero_keeps_only_system() {
    let strategy = SlidingWindowStrategy::new(0, 100_000);
    let messages = vec![
        system_msg("System prompt"),
        user_msg("msg1"),
        assistant_msg("msg2"),
        user_msg("msg3"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // Window size 0 → keep 0 non-system messages, only system
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, Role::System);
}

#[tokio::test]
async fn compact_exact_window_size_preserves_all_non_system() {
    let strategy = SlidingWindowStrategy::new(4, 100_000);
    let messages = vec![
        system_msg("System prompt"),
        user_msg("msg1"),
        assistant_msg("msg2"),
        user_msg("msg3"),
        assistant_msg("msg4"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // 1 system + 4 non-system (exactly window size) = 5
    assert_eq!(result.len(), 5);
}

#[tokio::test]
async fn compact_preserves_message_order() {
    let strategy = SlidingWindowStrategy::new(3, 100_000);
    let messages = vec![
        system_msg("sys"),
        user_msg("old1"),
        assistant_msg("old2"),
        user_msg("recent1"),
        assistant_msg("recent2"),
        user_msg("recent3"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // System + last 3 non-system
    assert_eq!(result.len(), 4);
    // System message first
    assert_eq!(result[0].role, Role::System);
    // Then user, assistant, user in order
    assert_eq!(result[1].role, Role::User);
    assert!(matches!(&result[1].content[0], ContentBlock::Text(t) if t == "recent1"));
    assert_eq!(result[2].role, Role::Assistant);
    assert!(matches!(&result[2].content[0], ContentBlock::Text(t) if t == "recent2"));
    assert_eq!(result[3].role, Role::User);
    assert!(matches!(&result[3].content[0], ContentBlock::Text(t) if t == "recent3"));
}

#[tokio::test]
async fn compact_window_larger_than_message_count() {
    let strategy = SlidingWindowStrategy::new(100, 100_000);
    let messages = vec![user_msg("one"), assistant_msg("two"), user_msg("three")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // All 3 non-system messages kept (window is larger)
    assert_eq!(result.len(), 3);
}

#[tokio::test]
async fn compact_single_non_system_message() {
    let strategy = SlidingWindowStrategy::new(5, 100_000);
    let messages = vec![system_msg("sys"), user_msg("only")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    assert_eq!(result.len(), 2);
}

#[test]
fn should_compact_exact_threshold_returns_false() {
    let strategy = SlidingWindowStrategy::new(5, 1000);
    let messages = vec![user_msg("hi")];
    // At exactly the threshold, should not compact (> not >=)
    assert!(!strategy.should_compact(&messages, 1000));
}

#[test]
fn token_estimate_empty_messages() {
    let strategy = SlidingWindowStrategy::new(5, 1000);
    assert_eq!(strategy.token_estimate(&[]), 0);
}

#[test]
fn with_counter_uses_custom_ratio() {
    let counter = neuron_context::TokenCounter::with_ratio(2.0);
    let strategy = SlidingWindowStrategy::with_counter(5, 1000, counter);
    let messages = vec![user_msg("a".repeat(20).as_str())];
    let estimate = strategy.token_estimate(&messages);
    // 4 (overhead) + ceil(20/2) = 4 + 10 = 14
    assert_eq!(estimate, 14);
}

#[tokio::test]
async fn compact_multiple_system_messages_scattered() {
    // System messages in various positions should all be collected at the front
    let strategy = SlidingWindowStrategy::new(2, 100_000);
    let messages = vec![
        system_msg("sys1"),
        user_msg("u1"),
        system_msg("sys2"),
        assistant_msg("a1"),
        user_msg("u2"),
        assistant_msg("a2"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // 2 system + last 2 non-system = 4
    assert_eq!(result.len(), 4);
    // First two should be system
    assert_eq!(result[0].role, Role::System);
    assert_eq!(result[1].role, Role::System);
    // Last two should be the most recent non-system messages
    assert!(matches!(&result[2].content[0], ContentBlock::Text(t) if t == "u2"));
    assert!(matches!(&result[3].content[0], ContentBlock::Text(t) if t == "a2"));
}
