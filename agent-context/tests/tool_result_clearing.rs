//! Integration tests for ToolResultClearingStrategy.

use agent_context::ToolResultClearingStrategy;
use agent_types::{ContentBlock, ContentItem, ContextStrategy, Message, Role};

fn user_msg(text: &str) -> Message {
    Message { role: Role::User, content: vec![ContentBlock::Text(text.to_string())] }
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

    let result = strategy.compact(messages).await.expect("compact should succeed");

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

    let result = strategy.compact(messages).await.expect("compact should succeed");

    // The cleared result should still have the original tool_use_id
    let tool_result_msgs: Vec<&Message> = result
        .iter()
        .filter(|m| m.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. })))
        .collect();

    assert_eq!(tool_result_msgs.len(), 2);
    // First is cleared but id preserved
    assert_eq!(extract_tool_result_id(tool_result_msgs[0]), Some("abc-123".to_string()));
    assert_eq!(extract_tool_result_content(tool_result_msgs[0]), Some("[tool result cleared]".to_string()));
    // Second is untouched
    assert_eq!(extract_tool_result_id(tool_result_msgs[1]), Some("def-456".to_string()));
    assert_eq!(extract_tool_result_content(tool_result_msgs[1]), Some("recent content".to_string()));
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

    let result = strategy.compact(messages).await.expect("compact should succeed");

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

    let result = strategy.compact(messages).await.expect("compact should succeed");

    // Both should be intact
    assert_eq!(extract_tool_result_content(&result[0]), Some("result a".to_string()));
    assert_eq!(extract_tool_result_content(&result[1]), Some("result b".to_string()));
}

#[test]
fn should_compact_uses_token_threshold() {
    let strategy = ToolResultClearingStrategy::new(2, 1000);
    let msgs = vec![user_msg("hi")];
    assert!(!strategy.should_compact(&msgs, 999));
    assert!(strategy.should_compact(&msgs, 1001));
}
