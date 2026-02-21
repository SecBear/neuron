use agent_types::*;

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
                source: ImageSource::Url { url: "https://example.com/img.png".into() },
            },
        ],
        is_error: false,
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::ToolResult { content, is_error, .. } = rt {
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
