use neuron_types::*;

#[test]
fn message_roundtrip_serde() {
    let msg = Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Text("hello".into()),
            ContentBlock::ToolUse {
                id: "t1".into(),
                name: "read_file".into(),
                input: serde_json::json!({"path": "/tmp/foo"}),
            },
        ],
    };
    let json = serde_json::to_string(&msg).unwrap();
    let roundtrip: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.role, Role::Assistant);
    assert_eq!(roundtrip.content.len(), 2);
}

#[test]
fn tool_result_with_content_items() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "t1".into(),
        content: vec![
            ContentItem::Text("file contents here".into()),
            ContentItem::Image {
                source: ImageSource::Url {
                    url: "https://example.com/img.png".into(),
                },
            },
        ],
        is_error: false,
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::ToolResult {
        content, is_error, ..
    } = rt
    {
        assert_eq!(content.len(), 2);
        assert!(!is_error);
    } else {
        panic!("expected ToolResult");
    }
}

#[test]
fn thinking_block_serde() {
    let block = ContentBlock::Thinking {
        thinking: "Let me consider...".into(),
        signature: "sig123".into(),
    };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains("thinking"));
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::Thinking { thinking, .. } = rt {
        assert_eq!(thinking, "Let me consider...");
    } else {
        panic!("expected Thinking");
    }
}

// --- Message convenience constructors with String (not just &str) ---

#[test]
fn message_user_accepts_string() {
    let text = String::from("Hello from a String");
    let msg = Message::user(text);
    assert_eq!(msg.role, Role::User);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello from a String"));
}

#[test]
fn message_assistant_accepts_string() {
    let text = String::from("Response from a String");
    let msg = Message::assistant(text);
    assert_eq!(msg.role, Role::Assistant);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Response from a String"));
}

#[test]
fn message_system_accepts_string() {
    let text = String::from("System from a String");
    let msg = Message::system(text);
    assert_eq!(msg.role, Role::System);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "System from a String"));
}

// --- ContentBlock variant serde roundtrips ---

#[test]
fn redacted_thinking_block_serde() {
    let block = ContentBlock::RedactedThinking {
        data: "opaque_blob_abc".into(),
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::RedactedThinking { data } = rt {
        assert_eq!(data, "opaque_blob_abc");
    } else {
        panic!("expected RedactedThinking");
    }
}

#[test]
fn compaction_block_serde() {
    let block = ContentBlock::Compaction {
        content: "Summary of prior conversation...".into(),
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::Compaction { content } = rt {
        assert_eq!(content, "Summary of prior conversation...");
    } else {
        panic!("expected Compaction");
    }
}

#[test]
fn image_block_base64_serde() {
    let block = ContentBlock::Image {
        source: ImageSource::Base64 {
            media_type: "image/png".into(),
            data: "iVBORw0KGgo=".into(),
        },
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::Image {
        source: ImageSource::Base64 { media_type, data },
    } = rt
    {
        assert_eq!(media_type, "image/png");
        assert_eq!(data, "iVBORw0KGgo=");
    } else {
        panic!("expected Image with Base64 source");
    }
}

#[test]
fn image_block_url_serde() {
    let block = ContentBlock::Image {
        source: ImageSource::Url {
            url: "https://example.com/image.jpg".into(),
        },
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::Image {
        source: ImageSource::Url { url },
    } = rt
    {
        assert_eq!(url, "https://example.com/image.jpg");
    } else {
        panic!("expected Image with Url source");
    }
}

#[test]
fn document_block_base64_pdf_serde() {
    let block = ContentBlock::Document {
        source: DocumentSource::Base64Pdf {
            data: "JVBERi0xLjQ=".into(),
        },
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::Document {
        source: DocumentSource::Base64Pdf { data },
    } = rt
    {
        assert_eq!(data, "JVBERi0xLjQ=");
    } else {
        panic!("expected Document with Base64Pdf source");
    }
}

#[test]
fn document_block_plain_text_serde() {
    let block = ContentBlock::Document {
        source: DocumentSource::PlainText {
            data: "Some plain text document content.".into(),
        },
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::Document {
        source: DocumentSource::PlainText { data },
    } = rt
    {
        assert_eq!(data, "Some plain text document content.");
    } else {
        panic!("expected Document with PlainText source");
    }
}

#[test]
fn document_block_url_serde() {
    let block = ContentBlock::Document {
        source: DocumentSource::Url {
            url: "https://example.com/doc.pdf".into(),
        },
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::Document {
        source: DocumentSource::Url { url },
    } = rt
    {
        assert_eq!(url, "https://example.com/doc.pdf");
    } else {
        panic!("expected Document with Url source");
    }
}

#[test]
fn tool_result_with_error_flag() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "t42".into(),
        content: vec![ContentItem::Text("file not found".into())],
        is_error: true,
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::ToolResult {
        tool_use_id,
        content,
        is_error,
    } = rt
    {
        assert_eq!(tool_use_id, "t42");
        assert_eq!(content.len(), 1);
        assert!(is_error);
    } else {
        panic!("expected ToolResult");
    }
}

#[test]
fn tool_use_block_serde() {
    let block = ContentBlock::ToolUse {
        id: "call_abc".into(),
        name: "search".into(),
        input: serde_json::json!({"query": "Rust async", "limit": 10}),
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::ToolUse { id, name, input } = rt {
        assert_eq!(id, "call_abc");
        assert_eq!(name, "search");
        assert_eq!(input["query"], "Rust async");
        assert_eq!(input["limit"], 10);
    } else {
        panic!("expected ToolUse");
    }
}

// --- ContentItem serde roundtrip ---

#[test]
fn content_item_text_serde() {
    let item = ContentItem::Text("hello world".into());
    let json = serde_json::to_string(&item).unwrap();
    let rt: ContentItem = serde_json::from_str(&json).unwrap();
    if let ContentItem::Text(t) = rt {
        assert_eq!(t, "hello world");
    } else {
        panic!("expected Text");
    }
}

#[test]
fn content_item_image_serde() {
    let item = ContentItem::Image {
        source: ImageSource::Base64 {
            media_type: "image/jpeg".into(),
            data: "/9j/4AAQ=".into(),
        },
    };
    let json = serde_json::to_string(&item).unwrap();
    let rt: ContentItem = serde_json::from_str(&json).unwrap();
    if let ContentItem::Image {
        source: ImageSource::Base64 { media_type, data },
    } = rt
    {
        assert_eq!(media_type, "image/jpeg");
        assert_eq!(data, "/9j/4AAQ=");
    } else {
        panic!("expected Image with Base64 source");
    }
}

// --- Role serde roundtrip ---

#[test]
fn role_user_serde() {
    let role = Role::User;
    let json = serde_json::to_string(&role).unwrap();
    let rt: Role = serde_json::from_str(&json).unwrap();
    assert_eq!(rt, Role::User);
}

#[test]
fn role_assistant_serde() {
    let role = Role::Assistant;
    let json = serde_json::to_string(&role).unwrap();
    let rt: Role = serde_json::from_str(&json).unwrap();
    assert_eq!(rt, Role::Assistant);
}

#[test]
fn role_system_serde() {
    let role = Role::System;
    let json = serde_json::to_string(&role).unwrap();
    let rt: Role = serde_json::from_str(&json).unwrap();
    assert_eq!(rt, Role::System);
}

// --- Message with empty content ---

#[test]
fn message_with_empty_content_serde() {
    let msg = Message {
        role: Role::User,
        content: vec![],
    };
    let json = serde_json::to_string(&msg).unwrap();
    let rt: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.role, Role::User);
    assert!(rt.content.is_empty());
}

// --- Message with multiple mixed content blocks ---

#[test]
fn message_with_mixed_content_blocks() {
    let msg = Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Thinking {
                thinking: "Let me reason...".into(),
                signature: "sig".into(),
            },
            ContentBlock::Text("Here is my answer.".into()),
            ContentBlock::ToolUse {
                id: "t1".into(),
                name: "calculator".into(),
                input: serde_json::json!({"expression": "2+2"}),
            },
        ],
    };
    let json = serde_json::to_string(&msg).unwrap();
    let rt: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.content.len(), 3);
    assert!(matches!(&rt.content[0], ContentBlock::Thinking { .. }));
    assert!(matches!(&rt.content[1], ContentBlock::Text(_)));
    assert!(matches!(&rt.content[2], ContentBlock::ToolUse { .. }));
}
