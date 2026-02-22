//! Integration tests for TokenCounter.

use neuron_context::TokenCounter;
use neuron_types::{ContentBlock, ContentItem, Message, Role, ToolDefinition};

fn text_message(role: Role, text: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

fn tool_use_message(id: &str, name: &str, input: serde_json::Value) -> Message {
    Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: id.to_string(),
            name: name.to_string(),
            input,
        }],
    }
}

fn tool_result_message(tool_use_id: &str, text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: vec![ContentItem::Text(text.to_string())],
            is_error: false,
        }],
    }
}

#[test]
fn estimate_text_returns_reasonable_values() {
    let counter = TokenCounter::new();
    // "Hello, world!" is 13 chars → ceil(13/4) = 4
    assert_eq!(counter.estimate_text("Hello, world!"), 4);
    // Empty string → 0
    assert_eq!(counter.estimate_text(""), 0);
    // 100 chars → 25
    let hundred = "a".repeat(100);
    assert_eq!(counter.estimate_text(&hundred), 25);
}

#[test]
fn estimate_messages_empty_returns_zero() {
    let counter = TokenCounter::new();
    assert_eq!(counter.estimate_messages(&[]), 0);
}

#[test]
fn estimate_messages_handles_text_blocks() {
    let counter = TokenCounter::new();
    let messages = vec![
        text_message(Role::User, "Hello there"), // 11 chars + 4 overhead
        text_message(Role::Assistant, "Hi!"),    // 3 chars + 4 overhead
    ];
    let estimate = counter.estimate_messages(&messages);
    // Should be positive and reasonable
    assert!(estimate > 0);
    assert!(estimate < 100);
}

#[test]
fn estimate_messages_handles_tool_use_blocks() {
    let counter = TokenCounter::new();
    let input = serde_json::json!({"query": "search term"});
    let messages = vec![tool_use_message("id1", "search", input)];
    let estimate = counter.estimate_messages(&messages);
    assert!(estimate > 0);
}

#[test]
fn estimate_messages_handles_tool_result_blocks() {
    let counter = TokenCounter::new();
    let messages = vec![tool_result_message("id1", "search results here")];
    let estimate = counter.estimate_messages(&messages);
    assert!(estimate > 0);
}

#[test]
fn estimate_messages_handles_multiple_block_types() {
    let counter = TokenCounter::new();
    let messages = vec![
        text_message(Role::User, "Please search for rust"),
        tool_use_message("id1", "search", serde_json::json!({"q": "rust"})),
        tool_result_message("id1", "Rust is a systems programming language"),
        text_message(Role::Assistant, "Rust is a great language!"),
    ];
    let estimate = counter.estimate_messages(&messages);
    assert!(estimate > 0);
}

#[test]
fn custom_ratio_changes_estimates() {
    let default_counter = TokenCounter::new(); // 4.0 chars/token
    let tight_counter = TokenCounter::with_ratio(2.0); // 2.0 chars/token
    let text = "a".repeat(40);

    let default_est = default_counter.estimate_text(&text);
    let tight_est = tight_counter.estimate_text(&text);

    // Tighter ratio (fewer chars per token) → more tokens estimated
    assert!(tight_est > default_est);
    assert_eq!(default_est, 10); // 40/4
    assert_eq!(tight_est, 20); // 40/2
}

#[test]
fn estimate_tools_returns_positive_for_non_empty() {
    let counter = TokenCounter::new();
    let tools = vec![ToolDefinition {
        name: "search".to_string(),
        title: None,
        description: "Search the web for information".to_string(),
        input_schema: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}}),
        output_schema: None,
        annotations: None,
        cache_control: None,
    }];
    let estimate = counter.estimate_tools(&tools);
    assert!(estimate > 0);
}

#[test]
fn estimate_tools_empty_returns_zero() {
    let counter = TokenCounter::new();
    assert_eq!(counter.estimate_tools(&[]), 0);
}
