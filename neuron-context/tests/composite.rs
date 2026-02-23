//! Integration tests for CompositeStrategy.

use neuron_context::{
    CompositeStrategy, SlidingWindowStrategy, ToolResultClearingStrategy, strategies::BoxedStrategy,
};
use neuron_types::{ContentBlock, ContentItem, ContextStrategy, Message, Role};

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

fn tool_result_msg(tool_use_id: &str, content: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: vec![ContentItem::Text(content.to_string())],
            is_error: false,
        }],
    }
}

fn tool_use_msg(id: &str) -> Message {
    Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: id.to_string(),
            name: "my_tool".to_string(),
            input: serde_json::json!({}),
        }],
    }
}

#[tokio::test]
async fn applies_first_strategy_when_sufficient() {
    // A composite that first clears tool results, then slides window.
    // With a very high max_tokens threshold for the outer composite,
    // compact is forced; but internally the first strategy alone may suffice.

    let strategy = CompositeStrategy::new(
        vec![
            BoxedStrategy::new(ToolResultClearingStrategy::new(1, 0)), // always compact
            BoxedStrategy::new(SlidingWindowStrategy::new(3, 0)),      // always compact
        ],
        0, // threshold 0 → always triggers should_compact
    );

    let messages = vec![
        user_msg("start"),
        tool_use_msg("id1"),
        tool_result_msg("id1", "first result"),
        tool_use_msg("id2"),
        tool_result_msg("id2", "second result"),
        assistant_msg("done"),
    ];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // Both strategies ran; tool results cleared, and window slid
    assert!(!result.is_empty());
}

#[tokio::test]
async fn stops_early_when_under_budget() {
    // The first strategy always brings the token count to 0 (conceptually),
    // so the second should never be needed. We verify this by using a sliding
    // window with a very small window_size that would truncate everything.

    // Use a realistic setup: first strategy clears tool results,
    // which may reduce tokens enough to skip the window strategy.

    let strategy = CompositeStrategy::new(
        vec![
            // Clears all tool results (keep_recent_n=0)
            BoxedStrategy::new(ToolResultClearingStrategy::new(0, 1)),
            // Window keeps only 1 message — would be very aggressive
            BoxedStrategy::new(SlidingWindowStrategy::new(1, 1)),
        ],
        // max_tokens = very large so after first compact, we're under budget
        usize::MAX,
    );

    let messages = vec![
        tool_use_msg("id1"),
        tool_result_msg("id1", "result"),
        user_msg("question"),
        assistant_msg("answer"),
    ];

    // compact is called manually (bypassing should_compact check)
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // With max_tokens=MAX, the loop exits immediately after checking token count
    // and neither inner strategy runs — messages pass through unchanged
    assert_eq!(result.len(), 4);
}

#[tokio::test]
async fn chaining_clearing_then_sliding_both_apply() {
    // Force both strategies to run by setting max_tokens=0 on both inner strategies
    // and usize::MAX on composite (so composite.compact runs both).
    // We set the composite threshold very low so should_compact returns true,
    // then compact loops through all strategies since tokens are always > 0.

    let strategy = CompositeStrategy::new(
        vec![
            // Keep only 1 recent tool result
            BoxedStrategy::new(ToolResultClearingStrategy::new(1, 0)),
            // Keep only 2 non-system messages
            BoxedStrategy::new(SlidingWindowStrategy::new(2, 0)),
        ],
        0, // threshold → always compact
    );

    let messages = vec![
        user_msg("msg1"),
        tool_use_msg("id1"),
        tool_result_msg("id1", "old result"),
        tool_use_msg("id2"),
        tool_result_msg("id2", "new result"),
        user_msg("msg2"),
        assistant_msg("msg3"),
        user_msg("msg4"),
    ];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // After tool clearing: old result cleared, new result kept
    // After window: only last 2 messages
    assert!(
        result.len() <= 4,
        "expected at most 4 messages, got {}",
        result.len()
    );
}

#[test]
fn should_compact_checks_threshold() {
    let strategy = CompositeStrategy::new(
        vec![BoxedStrategy::new(SlidingWindowStrategy::new(5, 100_000))],
        50_000,
    );
    let msgs = vec![user_msg("hi")];
    assert!(!strategy.should_compact(&msgs, 49_999));
    assert!(strategy.should_compact(&msgs, 50_001));
}

// ---- Additional coverage tests ----

#[tokio::test]
async fn empty_strategy_list_returns_messages_unchanged() {
    let strategy = CompositeStrategy::new(vec![], 100_000);
    let messages = vec![user_msg("hello"), assistant_msg("world")];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    assert_eq!(result.len(), 2);
    assert!(matches!(&result[0].content[0], ContentBlock::Text(t) if t == "hello"));
    assert!(matches!(&result[1].content[0], ContentBlock::Text(t) if t == "world"));
}

#[tokio::test]
async fn empty_messages_with_strategies() {
    let strategy = CompositeStrategy::new(
        vec![BoxedStrategy::new(SlidingWindowStrategy::new(5, 0))],
        0,
    );
    let result = strategy
        .compact(vec![])
        .await
        .expect("compact should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn single_strategy_applies_normally() {
    let strategy = CompositeStrategy::new(
        vec![BoxedStrategy::new(SlidingWindowStrategy::new(2, 0))],
        0,
    );
    let messages = vec![
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
    // SlidingWindow keeps last 2
    assert_eq!(result.len(), 2);
}

#[test]
fn should_compact_exact_threshold_returns_false() {
    let strategy = CompositeStrategy::new(
        vec![BoxedStrategy::new(SlidingWindowStrategy::new(5, 100_000))],
        50_000,
    );
    let msgs = vec![user_msg("hi")];
    // Exactly at threshold → not over
    assert!(!strategy.should_compact(&msgs, 50_000));
}

#[test]
fn token_estimate_delegates_to_counter() {
    let strategy = CompositeStrategy::new(
        vec![BoxedStrategy::new(SlidingWindowStrategy::new(5, 100_000))],
        50_000,
    );
    let messages = vec![user_msg("hello world")];
    assert!(strategy.token_estimate(&messages) > 0);
}

#[test]
fn token_estimate_empty_messages() {
    let strategy = CompositeStrategy::new(vec![], 50_000);
    assert_eq!(strategy.token_estimate(&[]), 0);
}

// ---- BoxedStrategy tests ----

#[test]
fn boxed_strategy_should_compact_delegates() {
    let inner = SlidingWindowStrategy::new(5, 1000);
    let boxed = BoxedStrategy::new(inner);
    let msgs = vec![user_msg("hi")];
    assert!(!boxed.should_compact(&msgs, 999));
    assert!(boxed.should_compact(&msgs, 1001));
}

#[test]
fn boxed_strategy_token_estimate_delegates() {
    let inner = SlidingWindowStrategy::new(5, 1000);
    let boxed = BoxedStrategy::new(inner);
    let messages = vec![user_msg("hello world")];
    let expected = SlidingWindowStrategy::new(5, 1000).token_estimate(&messages);
    assert_eq!(boxed.token_estimate(&messages), expected);
}

#[tokio::test]
async fn boxed_strategy_compact_delegates() {
    let inner = SlidingWindowStrategy::new(2, 1000);
    let boxed = BoxedStrategy::new(inner);
    let messages = vec![
        user_msg("msg1"),
        assistant_msg("msg2"),
        user_msg("msg3"),
        assistant_msg("msg4"),
        user_msg("msg5"),
    ];
    let result = boxed
        .compact(messages)
        .await
        .expect("compact should succeed");
    // SlidingWindow keeps last 2
    assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn boxed_tool_result_clearing_strategy() {
    let inner = ToolResultClearingStrategy::new(0, 100_000);
    let boxed = BoxedStrategy::new(inner);
    let messages = vec![tool_use_msg("id1"), tool_result_msg("id1", "result data")];
    let result = boxed
        .compact(messages)
        .await
        .expect("compact should succeed");
    // Tool result should be cleared
    let tool_result = result
        .iter()
        .find(|m| {
            m.content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
        })
        .expect("should have a tool result");
    if let ContentBlock::ToolResult { content, .. } = &tool_result.content[0] {
        assert!(matches!(&content[0], ContentItem::Text(t) if t == "[tool result cleared]"));
    }
}

#[tokio::test]
async fn composite_three_strategies_all_apply() {
    // Three strategies, all with threshold 0
    let strategy = CompositeStrategy::new(
        vec![
            BoxedStrategy::new(ToolResultClearingStrategy::new(0, 0)),
            BoxedStrategy::new(ToolResultClearingStrategy::new(0, 0)), // second pass (no-op)
            BoxedStrategy::new(SlidingWindowStrategy::new(1, 0)),
        ],
        0,
    );
    let messages = vec![
        user_msg("msg1"),
        tool_use_msg("id1"),
        tool_result_msg("id1", "result"),
        user_msg("msg2"),
        assistant_msg("msg3"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // After sliding window with size 1, only last message kept
    assert_eq!(result.len(), 1);
}
