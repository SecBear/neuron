//! Integration tests for TokenCounter.

use neuron_context::TokenCounter;
use neuron_types::{
    ContentBlock, ContentItem, DocumentSource, ImageSource, Message, Role, ToolDefinition,
};

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

// ---- Additional coverage tests ----

#[test]
fn default_is_same_as_new() {
    let default_counter = TokenCounter::default();
    let new_counter = TokenCounter::new();
    let text = "Some sample text for estimation";
    assert_eq!(
        default_counter.estimate_text(text),
        new_counter.estimate_text(text)
    );
}

#[test]
fn estimate_image_block_returns_fixed_300() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Image {
            source: ImageSource::Base64 {
                media_type: "image/png".to_string(),
                data: "iVBORw0KGgo=".to_string(),
            },
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 300 (image fixed) = 304
    assert_eq!(estimate, 304);
}

#[test]
fn estimate_image_url_source_returns_fixed_300() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Image {
            source: ImageSource::Url {
                url: "https://example.com/image.png".to_string(),
            },
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    assert_eq!(estimate, 304);
}

#[test]
fn estimate_document_block_returns_fixed_500() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Document {
            source: DocumentSource::Base64Pdf {
                data: "JVBERi0xLjQ=".to_string(),
            },
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 500 (document fixed) = 504
    assert_eq!(estimate, 504);
}

#[test]
fn estimate_document_plain_text_source() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Document {
            source: DocumentSource::PlainText {
                data: "some text".to_string(),
            },
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    assert_eq!(estimate, 504);
}

#[test]
fn estimate_document_url_source() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Document {
            source: DocumentSource::Url {
                url: "https://example.com/doc.pdf".to_string(),
            },
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    assert_eq!(estimate, 504);
}

#[test]
fn estimate_compaction_block_uses_text_estimation() {
    let counter = TokenCounter::new();
    let compaction_text = "a".repeat(40); // 40 chars = 10 tokens at ratio 4.0
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Compaction {
            content: compaction_text,
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 10 (40/4) = 14
    assert_eq!(estimate, 14);
}

#[test]
fn estimate_thinking_block_uses_thinking_text() {
    let counter = TokenCounter::new();
    let thinking_text = "a".repeat(20); // 20 chars = 5 tokens
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Thinking {
            thinking: thinking_text,
            signature: "sig-abc".to_string(),
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 5 (20/4) = 9
    assert_eq!(estimate, 9);
}

#[test]
fn estimate_redacted_thinking_block_uses_data_text() {
    let counter = TokenCounter::new();
    let data = "a".repeat(16); // 16 chars = 4 tokens
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![ContentBlock::RedactedThinking { data }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 4 (16/4) = 8
    assert_eq!(estimate, 8);
}

#[test]
fn estimate_multiple_blocks_in_single_message() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![
            ContentBlock::Text("a".repeat(40).to_string()), // 10 tokens
            ContentBlock::Image {
                source: ImageSource::Url {
                    url: "https://example.com/img.png".to_string(),
                },
            }, // 300 tokens
            ContentBlock::Text("b".repeat(8).to_string()),  // 2 tokens
        ],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 10 + 300 + 2 = 316
    assert_eq!(estimate, 316);
}

#[test]
fn estimate_tool_result_with_image_content_item() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::ToolResult {
            tool_use_id: "id1".to_string(),
            content: vec![ContentItem::Image {
                source: ImageSource::Base64 {
                    media_type: "image/jpeg".to_string(),
                    data: "base64data".to_string(),
                },
            }],
            is_error: false,
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 300 (image content item) = 304
    assert_eq!(estimate, 304);
}

#[test]
fn estimate_tool_result_with_mixed_content_items() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::ToolResult {
            tool_use_id: "id1".to_string(),
            content: vec![
                ContentItem::Text("a".repeat(20).to_string()), // 5 tokens
                ContentItem::Image {
                    source: ImageSource::Url {
                        url: "https://example.com/img.png".to_string(),
                    },
                }, // 300 tokens
            ],
            is_error: false,
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 5 + 300 = 309
    assert_eq!(estimate, 309);
}

#[test]
fn estimate_tool_result_with_empty_content() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::ToolResult {
            tool_use_id: "id1".to_string(),
            content: vec![],
            is_error: false,
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 0 (empty content) = 4
    assert_eq!(estimate, 4);
}

#[test]
fn estimate_tool_use_block_includes_name_and_input() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: "call-1".to_string(),
            name: "a".repeat(8).to_string(), // 8 chars = 2 tokens
            input: serde_json::json!({}),    // "{}" = 2 chars = 1 token
        }],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead) + 2 (name) + 1 (input "{}") = 7
    assert_eq!(estimate, 7);
}

#[test]
fn estimate_message_with_empty_content_vec() {
    let counter = TokenCounter::new();
    let messages = vec![Message {
        role: Role::User,
        content: vec![],
    }];
    let estimate = counter.estimate_messages(&messages);
    // 4 (role overhead only)
    assert_eq!(estimate, 4);
}

#[test]
fn estimate_tools_multiple_tools() {
    let counter = TokenCounter::new();
    let tools = vec![
        ToolDefinition {
            name: "search".to_string(),
            title: None,
            description: "Search the web".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            cache_control: None,
        },
        ToolDefinition {
            name: "calculate".to_string(),
            title: None,
            description: "Perform calculations".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"expr": {"type": "string"}}}),
            output_schema: None,
            annotations: None,
            cache_control: None,
        },
    ];
    let estimate = counter.estimate_tools(&tools);
    // Both tools should contribute positive tokens
    let tool1_estimate = counter.estimate_tools(&tools[..1]);
    let tool2_estimate = counter.estimate_tools(&tools[1..]);
    assert_eq!(estimate, tool1_estimate + tool2_estimate);
}

#[test]
fn estimate_text_single_char() {
    let counter = TokenCounter::new();
    // 1 char → ceil(1/4) = 1
    assert_eq!(counter.estimate_text("x"), 1);
}

#[test]
fn estimate_text_exact_ratio_boundary() {
    let counter = TokenCounter::new();
    // 4 chars → ceil(4/4) = 1
    assert_eq!(counter.estimate_text("abcd"), 1);
    // 5 chars → ceil(5/4) = 2
    assert_eq!(counter.estimate_text("abcde"), 2);
}

#[test]
fn with_ratio_very_small_ratio() {
    let counter = TokenCounter::with_ratio(1.0);
    // Each char = 1 token
    assert_eq!(counter.estimate_text("hello"), 5);
}

#[test]
fn with_ratio_large_ratio() {
    let counter = TokenCounter::with_ratio(100.0);
    // 100 chars = 1 token
    let text = "a".repeat(100);
    assert_eq!(counter.estimate_text(&text), 1);
    // 101 chars = 2 tokens
    let text2 = "a".repeat(101);
    assert_eq!(counter.estimate_text(&text2), 2);
}
