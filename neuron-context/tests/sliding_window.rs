//! Integration tests for SlidingWindowStrategy.

use neuron_context::SlidingWindowStrategy;
use neuron_types::{ContentBlock, ContextStrategy, Message, Role};

fn user_msg(text: &str) -> Message {
    Message { role: Role::User, content: vec![ContentBlock::Text(text.to_string())] }
}

fn assistant_msg(text: &str) -> Message {
    Message { role: Role::Assistant, content: vec![ContentBlock::Text(text.to_string())] }
}

fn system_msg(text: &str) -> Message {
    Message { role: Role::System, content: vec![ContentBlock::Text(text.to_string())] }
}

#[tokio::test]
async fn keeps_only_last_n_non_system_messages() {
    let strategy = SlidingWindowStrategy::new(5, 100_000);

    // 10 non-system messages
    let mut messages: Vec<Message> = (0..10).map(|i| user_msg(&format!("message {i}"))).collect();
    messages.push(assistant_msg("final reply"));
    // total = 11 non-system

    let result = strategy.compact(messages).await.expect("compact should succeed");

    // Should keep only the last 5
    assert_eq!(result.len(), 5);
    // The last message should be retained
    assert!(matches!(&result.last().unwrap().content[0], ContentBlock::Text(t) if t == "final reply"));
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

    let result = strategy.compact(messages).await.expect("compact should succeed");

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
    let result = strategy.compact(messages).await.expect("compact should succeed");
    assert_eq!(result.len(), 2);
}

#[test]
fn token_estimate_delegates_to_counter() {
    let strategy = SlidingWindowStrategy::new(5, 1000);
    let messages = vec![user_msg("hello world")];
    // Non-zero estimate
    assert!(strategy.token_estimate(&messages) > 0);
}
