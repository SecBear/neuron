//! Integration tests for ToolResultClearingStrategy.

use neuron_context::ToolResultClearingStrategy;
use neuron_types::{ContentBlock, ContentItem, ContextStrategy, Message, Role};

fn user_msg(text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

fn assistant_msg_with_tool_use(id: &str, name: &str) -> Message {
    Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: id.to_string(),
            name: name.to_string(),
            input: serde_json::json!({"query": "test"}),
        }],
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

fn extract_tool_result_content(msg: &Message) -> Option<String> {
    msg.content.iter().find_map(|b| {
        if let ContentBlock::ToolResult { content, .. } = b {
            content.iter().find_map(|c| {
                if let ContentItem::Text(t) = c {
                    Some(t.clone())
                } else {
                    None
                }
            })
        } else {
            None
        }
    })
}

fn extract_tool_result_id(msg: &Message) -> Option<String> {
    msg.content.iter().find_map(|b| {
        if let ContentBlock::ToolResult { tool_use_id, .. } = b {
            Some(tool_use_id.clone())
        } else {
            None
        }
    })
}

#[tokio::test]
async fn clears_old_tool_results_keeps_recent() {
    let strategy = ToolResultClearingStrategy::new(2, 100_000);

    // 5 tool results
    let messages = vec![
        user_msg("start"),
        assistant_msg_with_tool_use("id1", "tool_a"),
        tool_result_msg("id1", "result one"),
        assistant_msg_with_tool_use("id2", "tool_b"),
        tool_result_msg("id2", "result two"),
        assistant_msg_with_tool_use("id3", "tool_c"),
        tool_result_msg("id3", "result three"),
        assistant_msg_with_tool_use("id4", "tool_d"),
        tool_result_msg("id4", "result four"),
        assistant_msg_with_tool_use("id5", "tool_e"),
        tool_result_msg("id5", "result five"),
    ];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // Collect all tool result contents
    let tool_result_contents: Vec<String> = result
        .iter()
        .filter_map(|m| extract_tool_result_content(m))
        .collect();

    assert_eq!(tool_result_contents.len(), 5);

    // First 3 should be cleared
    assert_eq!(tool_result_contents[0], "[tool result cleared]");
    assert_eq!(tool_result_contents[1], "[tool result cleared]");
    assert_eq!(tool_result_contents[2], "[tool result cleared]");
    // Last 2 should be kept
    assert_eq!(tool_result_contents[3], "result four");
    assert_eq!(tool_result_contents[4], "result five");
}

#[tokio::test]
async fn tool_use_ids_are_preserved_after_clearing() {
    let strategy = ToolResultClearingStrategy::new(1, 100_000);

    let messages = vec![
        assistant_msg_with_tool_use("abc-123", "tool_x"),
        tool_result_msg("abc-123", "original content"),
        assistant_msg_with_tool_use("def-456", "tool_y"),
        tool_result_msg("def-456", "recent content"),
    ];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // The cleared result should still have the original tool_use_id
    let tool_result_msgs: Vec<&Message> = result
        .iter()
        .filter(|m| {
            m.content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
        })
        .collect();

    assert_eq!(tool_result_msgs.len(), 2);
    // First is cleared but id preserved
    assert_eq!(
        extract_tool_result_id(tool_result_msgs[0]),
        Some("abc-123".to_string())
    );
    assert_eq!(
        extract_tool_result_content(tool_result_msgs[0]),
        Some("[tool result cleared]".to_string())
    );
    // Second is untouched
    assert_eq!(
        extract_tool_result_id(tool_result_msgs[1]),
        Some("def-456".to_string())
    );
    assert_eq!(
        extract_tool_result_content(tool_result_msgs[1]),
        Some("recent content".to_string())
    );
}

#[tokio::test]
async fn non_tool_result_messages_are_untouched() {
    let strategy = ToolResultClearingStrategy::new(0, 100_000);

    let messages = vec![
        user_msg("hello"),
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("I'll help you".to_string())],
        },
        tool_result_msg("id1", "some result"),
    ];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // Non-tool-result messages should be untouched
    let first = &result[0];
    assert_eq!(first.role, Role::User);
    assert!(matches!(&first.content[0], ContentBlock::Text(t) if t == "hello"));

    let second = &result[1];
    assert!(matches!(&second.content[0], ContentBlock::Text(t) if t == "I'll help you"));
}

#[tokio::test]
async fn fewer_results_than_keep_n_leaves_all_intact() {
    let strategy = ToolResultClearingStrategy::new(5, 100_000);

    let messages = vec![
        tool_result_msg("id1", "result a"),
        tool_result_msg("id2", "result b"),
    ];

    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");

    // Both should be intact
    assert_eq!(
        extract_tool_result_content(&result[0]),
        Some("result a".to_string())
    );
    assert_eq!(
        extract_tool_result_content(&result[1]),
        Some("result b".to_string())
    );
}

#[test]
fn should_compact_uses_token_threshold() {
    let strategy = ToolResultClearingStrategy::new(2, 1000);
    let msgs = vec![user_msg("hi")];
    assert!(!strategy.should_compact(&msgs, 999));
    assert!(strategy.should_compact(&msgs, 1001));
}

// ---- Additional coverage tests ----

#[tokio::test]
async fn compact_with_no_tool_results_returns_unchanged() {
    let strategy = ToolResultClearingStrategy::new(2, 100_000);
    let messages = vec![
        user_msg("hello"),
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("world".to_string())],
        },
        user_msg("goodbye"),
    ];
    let result = strategy
        .compact(messages.clone())
        .await
        .expect("compact should succeed");
    assert_eq!(result.len(), 3);
    // All messages should be structurally unchanged
    assert!(matches!(&result[0].content[0], ContentBlock::Text(t) if t == "hello"));
    assert!(matches!(&result[1].content[0], ContentBlock::Text(t) if t == "world"));
    assert!(matches!(&result[2].content[0], ContentBlock::Text(t) if t == "goodbye"));
}

#[tokio::test]
async fn compact_empty_messages_returns_empty() {
    let strategy = ToolResultClearingStrategy::new(2, 100_000);
    let result = strategy
        .compact(vec![])
        .await
        .expect("compact should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn keep_recent_zero_clears_all_tool_results() {
    let strategy = ToolResultClearingStrategy::new(0, 100_000);
    let messages = vec![
        assistant_msg_with_tool_use("id1", "tool_a"),
        tool_result_msg("id1", "result one"),
        assistant_msg_with_tool_use("id2", "tool_b"),
        tool_result_msg("id2", "result two"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    let tool_contents: Vec<String> = result
        .iter()
        .filter_map(|m| extract_tool_result_content(m))
        .collect();
    // Both cleared
    assert_eq!(tool_contents.len(), 2);
    assert!(tool_contents.iter().all(|c| c == "[tool result cleared]"));
}

#[tokio::test]
async fn clearing_error_tool_results_resets_is_error_flag() {
    let strategy = ToolResultClearingStrategy::new(0, 100_000);
    let messages = vec![
        assistant_msg_with_tool_use("id1", "tool_a"),
        Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "id1".to_string(),
                content: vec![ContentItem::Text("Error: something went wrong".to_string())],
                is_error: true,
            }],
        },
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // Find the tool result and check is_error was reset to false
    let tool_result_msg = result
        .iter()
        .find(|m| {
            m.content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
        })
        .expect("should have tool result message");
    if let ContentBlock::ToolResult { is_error, .. } = &tool_result_msg.content[0] {
        assert!(
            !is_error,
            "is_error should be reset to false after clearing"
        );
    } else {
        panic!("expected ToolResult block");
    }
}

#[tokio::test]
async fn multiple_tool_results_in_single_message() {
    let strategy = ToolResultClearingStrategy::new(1, 100_000);
    let messages = vec![
        // A message with two tool results
        Message {
            role: Role::User,
            content: vec![
                ContentBlock::ToolResult {
                    tool_use_id: "id1".to_string(),
                    content: vec![ContentItem::Text("first result".to_string())],
                    is_error: false,
                },
                ContentBlock::ToolResult {
                    tool_use_id: "id2".to_string(),
                    content: vec![ContentItem::Text("second result".to_string())],
                    is_error: false,
                },
            ],
        },
        // A third tool result in a separate message
        Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "id3".to_string(),
                content: vec![ContentItem::Text("third result".to_string())],
                is_error: false,
            }],
        },
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // 3 tool results total, keep 1 → clear first 2
    // First message has 2 tool results, both should be cleared
    let msg0 = &result[0];
    if let ContentBlock::ToolResult {
        content,
        tool_use_id,
        ..
    } = &msg0.content[0]
    {
        assert_eq!(tool_use_id, "id1");
        assert!(matches!(&content[0], ContentItem::Text(t) if t == "[tool result cleared]"));
    }
    if let ContentBlock::ToolResult {
        content,
        tool_use_id,
        ..
    } = &msg0.content[1]
    {
        assert_eq!(tool_use_id, "id2");
        assert!(matches!(&content[0], ContentItem::Text(t) if t == "[tool result cleared]"));
    }
    // Third result in second message should be kept
    let msg1 = &result[1];
    if let ContentBlock::ToolResult { content, .. } = &msg1.content[0] {
        assert!(matches!(&content[0], ContentItem::Text(t) if t == "third result"));
    }
}

#[test]
fn should_compact_exact_threshold_returns_false() {
    let strategy = ToolResultClearingStrategy::new(2, 1000);
    let msgs = vec![user_msg("hi")];
    // Exactly at threshold → not over, so false
    assert!(!strategy.should_compact(&msgs, 1000));
}

#[test]
fn token_estimate_delegates_to_counter() {
    let strategy = ToolResultClearingStrategy::new(2, 1000);
    let messages = vec![user_msg("hello world")];
    assert!(strategy.token_estimate(&messages) > 0);
}

#[test]
fn token_estimate_empty_messages() {
    let strategy = ToolResultClearingStrategy::new(2, 1000);
    assert_eq!(strategy.token_estimate(&[]), 0);
}

#[test]
fn with_counter_uses_custom_ratio() {
    let counter = neuron_context::TokenCounter::with_ratio(2.0);
    let strategy = ToolResultClearingStrategy::with_counter(2, 1000, counter);
    let messages = vec![user_msg("a".repeat(20).as_str())];
    let estimate = strategy.token_estimate(&messages);
    // 4 (overhead) + ceil(20/2) = 14
    assert_eq!(estimate, 14);
}

#[tokio::test]
async fn single_tool_result_with_keep_one_preserves_it() {
    let strategy = ToolResultClearingStrategy::new(1, 100_000);
    let messages = vec![
        assistant_msg_with_tool_use("id1", "tool_a"),
        tool_result_msg("id1", "only result"),
    ];
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    assert_eq!(
        extract_tool_result_content(&result[1]),
        Some("only result".to_string())
    );
}

#[tokio::test]
async fn total_message_count_unchanged_after_clearing() {
    let strategy = ToolResultClearingStrategy::new(1, 100_000);
    let messages = vec![
        user_msg("hello"),
        assistant_msg_with_tool_use("id1", "t1"),
        tool_result_msg("id1", "r1"),
        assistant_msg_with_tool_use("id2", "t2"),
        tool_result_msg("id2", "r2"),
        user_msg("bye"),
    ];
    let original_count = messages.len();
    let result = strategy
        .compact(messages)
        .await
        .expect("compact should succeed");
    // Clearing replaces content in-place, so message count should be same
    assert_eq!(result.len(), original_count);
}
