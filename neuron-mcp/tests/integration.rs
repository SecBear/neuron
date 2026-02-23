//! Integration tests for neuron-mcp.
//!
//! Note: Tests that require a running MCP server are ignored by default.
//! Run them with `cargo test -- --ignored` when a server is available.

use neuron_mcp::*;

#[test]
fn paginated_list_creation() {
    let list = PaginatedList {
        items: vec!["a".to_string(), "b".to_string()],
        next_cursor: Some("cursor_123".to_string()),
    };

    assert_eq!(list.items.len(), 2);
    assert_eq!(list.next_cursor, Some("cursor_123".to_string()));
}

#[test]
fn paginated_list_no_cursor() {
    let list: PaginatedList<i32> = PaginatedList {
        items: vec![1, 2, 3],
        next_cursor: None,
    };

    assert_eq!(list.items.len(), 3);
    assert!(list.next_cursor.is_none());
}

#[test]
fn mcp_resource_serialization() {
    let resource = McpResource {
        uri: "file:///test.txt".to_string(),
        name: "test".to_string(),
        title: Some("Test File".to_string()),
        description: Some("A test file".to_string()),
        mime_type: Some("text/plain".to_string()),
    };

    let json = serde_json::to_value(&resource).expect("should serialize");
    assert_eq!(json["uri"], "file:///test.txt");
    assert_eq!(json["name"], "test");
    assert_eq!(json["title"], "Test File");
}

#[test]
fn mcp_resource_contents_text() {
    let contents = McpResourceContents {
        uri: "file:///test.txt".to_string(),
        mime_type: Some("text/plain".to_string()),
        text: Some("hello world".to_string()),
        blob: None,
    };

    assert!(contents.text.is_some());
    assert!(contents.blob.is_none());
}

#[test]
fn mcp_resource_contents_blob() {
    let contents = McpResourceContents {
        uri: "file:///image.png".to_string(),
        mime_type: Some("image/png".to_string()),
        text: None,
        blob: Some("iVBOR...".to_string()),
    };

    assert!(contents.text.is_none());
    assert!(contents.blob.is_some());
}

#[test]
fn mcp_prompt_serialization() {
    let prompt = McpPrompt {
        name: "greet".to_string(),
        title: Some("Greeting Prompt".to_string()),
        description: Some("Generates a greeting".to_string()),
        arguments: vec![
            McpPromptArgument {
                name: "name".to_string(),
                description: Some("Person to greet".to_string()),
                required: Some(true),
            },
            McpPromptArgument {
                name: "style".to_string(),
                description: Some("Greeting style".to_string()),
                required: Some(false),
            },
        ],
    };

    let json = serde_json::to_value(&prompt).expect("should serialize");
    assert_eq!(json["name"], "greet");
    assert_eq!(json["arguments"].as_array().expect("array").len(), 2);
}

#[test]
fn mcp_prompt_no_args() {
    let prompt = McpPrompt {
        name: "simple".to_string(),
        title: None,
        description: None,
        arguments: vec![],
    };

    assert_eq!(prompt.name, "simple");
    assert!(prompt.arguments.is_empty());
}

#[test]
fn mcp_prompt_message_text() {
    let msg = McpPromptMessage {
        role: "user".to_string(),
        content: McpPromptContent::Text("Hello!".to_string()),
    };

    match &msg.content {
        McpPromptContent::Text(t) => assert_eq!(t, "Hello!"),
        _ => panic!("expected text content"),
    }
}

#[test]
fn mcp_prompt_result() {
    let result = McpPromptResult {
        description: Some("A test prompt".to_string()),
        messages: vec![McpPromptMessage {
            role: "assistant".to_string(),
            content: McpPromptContent::Text("Hi there!".to_string()),
        }],
    };

    assert_eq!(result.messages.len(), 1);
    assert!(result.description.is_some());
}

// --- Additional coverage tests ---

#[test]
fn mcp_prompt_content_image_variant() {
    let content = McpPromptContent::Image {
        data: "base64data".to_string(),
        mime_type: "image/png".to_string(),
    };

    match &content {
        McpPromptContent::Image { data, mime_type } => {
            assert_eq!(data, "base64data");
            assert_eq!(mime_type, "image/png");
        }
        _ => panic!("expected image content"),
    }
}

#[test]
fn mcp_prompt_content_resource_variant() {
    let content = McpPromptContent::Resource {
        uri: "file:///doc.txt".to_string(),
        mime_type: Some("text/plain".to_string()),
        text: Some("resource text".to_string()),
    };

    match &content {
        McpPromptContent::Resource {
            uri,
            mime_type,
            text,
        } => {
            assert_eq!(uri, "file:///doc.txt");
            assert_eq!(mime_type.as_deref(), Some("text/plain"));
            assert_eq!(text.as_deref(), Some("resource text"));
        }
        _ => panic!("expected resource content"),
    }
}

#[test]
fn mcp_prompt_content_resource_minimal() {
    let content = McpPromptContent::Resource {
        uri: "file:///minimal".to_string(),
        mime_type: None,
        text: None,
    };

    match &content {
        McpPromptContent::Resource {
            uri,
            mime_type,
            text,
        } => {
            assert_eq!(uri, "file:///minimal");
            assert!(mime_type.is_none());
            assert!(text.is_none());
        }
        _ => panic!("expected resource content"),
    }
}

#[test]
fn mcp_prompt_content_image_serialization_roundtrip() {
    let content = McpPromptContent::Image {
        data: "iVBORw0K".to_string(),
        mime_type: "image/jpeg".to_string(),
    };

    let json = serde_json::to_string(&content).expect("should serialize");
    let deserialized: McpPromptContent = serde_json::from_str(&json).expect("should deserialize");

    match deserialized {
        McpPromptContent::Image { data, mime_type } => {
            assert_eq!(data, "iVBORw0K");
            assert_eq!(mime_type, "image/jpeg");
        }
        _ => panic!("expected image after roundtrip"),
    }
}

#[test]
fn mcp_prompt_content_resource_serialization_roundtrip() {
    let content = McpPromptContent::Resource {
        uri: "file:///test".to_string(),
        mime_type: Some("application/json".to_string()),
        text: Some("{\"key\": \"value\"}".to_string()),
    };

    let json = serde_json::to_string(&content).expect("should serialize");
    let deserialized: McpPromptContent = serde_json::from_str(&json).expect("should deserialize");

    match deserialized {
        McpPromptContent::Resource {
            uri,
            mime_type,
            text,
        } => {
            assert_eq!(uri, "file:///test");
            assert_eq!(mime_type, Some("application/json".to_string()));
            assert!(text.is_some());
        }
        _ => panic!("expected resource after roundtrip"),
    }
}

#[test]
fn mcp_prompt_content_text_serialization_roundtrip() {
    let content = McpPromptContent::Text("Hello, world!".to_string());

    let json = serde_json::to_string(&content).expect("should serialize");
    let deserialized: McpPromptContent = serde_json::from_str(&json).expect("should deserialize");

    match deserialized {
        McpPromptContent::Text(t) => assert_eq!(t, "Hello, world!"),
        _ => panic!("expected text after roundtrip"),
    }
}

#[test]
fn paginated_list_serde_roundtrip() {
    let list = PaginatedList {
        items: vec![1, 2, 3],
        next_cursor: Some("page2".to_string()),
    };

    let json = serde_json::to_string(&list).expect("should serialize");
    let deserialized: PaginatedList<i32> = serde_json::from_str(&json).expect("should deserialize");

    assert_eq!(deserialized.items, vec![1, 2, 3]);
    assert_eq!(deserialized.next_cursor, Some("page2".to_string()));
}

#[test]
fn paginated_list_empty() {
    let list: PaginatedList<String> = PaginatedList {
        items: vec![],
        next_cursor: None,
    };

    assert!(list.items.is_empty());
    assert!(list.next_cursor.is_none());

    let json = serde_json::to_string(&list).expect("should serialize");
    let deserialized: PaginatedList<String> =
        serde_json::from_str(&json).expect("should deserialize");
    assert!(deserialized.items.is_empty());
}

#[test]
fn mcp_resource_deserialization() {
    let json = r#"{
        "uri": "http://example.com/data",
        "name": "data",
        "title": null,
        "description": null,
        "mime_type": "application/json"
    }"#;

    let resource: McpResource = serde_json::from_str(json).expect("should deserialize");
    assert_eq!(resource.uri, "http://example.com/data");
    assert_eq!(resource.name, "data");
    assert!(resource.title.is_none());
    assert!(resource.description.is_none());
    assert_eq!(resource.mime_type, Some("application/json".to_string()));
}

#[test]
fn mcp_resource_minimal() {
    let resource = McpResource {
        uri: "urn:test".to_string(),
        name: "minimal".to_string(),
        title: None,
        description: None,
        mime_type: None,
    };

    let json = serde_json::to_value(&resource).expect("should serialize");
    assert_eq!(json["uri"], "urn:test");
    assert!(json["title"].is_null());
    assert!(json["mime_type"].is_null());
}

#[test]
fn mcp_resource_contents_serialization_roundtrip() {
    let contents = McpResourceContents {
        uri: "file:///data.json".to_string(),
        mime_type: Some("application/json".to_string()),
        text: Some("{\"hello\": \"world\"}".to_string()),
        blob: None,
    };

    let json = serde_json::to_string(&contents).expect("should serialize");
    let deserialized: McpResourceContents =
        serde_json::from_str(&json).expect("should deserialize");

    assert_eq!(deserialized.uri, contents.uri);
    assert_eq!(deserialized.mime_type, contents.mime_type);
    assert_eq!(deserialized.text, contents.text);
    assert!(deserialized.blob.is_none());
}

#[test]
fn mcp_resource_contents_blob_roundtrip() {
    let contents = McpResourceContents {
        uri: "file:///binary.bin".to_string(),
        mime_type: Some("application/octet-stream".to_string()),
        text: None,
        blob: Some("SGVsbG8=".to_string()),
    };

    let json = serde_json::to_string(&contents).expect("should serialize");
    let deserialized: McpResourceContents =
        serde_json::from_str(&json).expect("should deserialize");

    assert!(deserialized.text.is_none());
    assert_eq!(deserialized.blob, Some("SGVsbG8=".to_string()));
}

#[test]
fn mcp_prompt_argument_optional_fields() {
    let arg = McpPromptArgument {
        name: "optfield".to_string(),
        description: None,
        required: None,
    };

    let json = serde_json::to_value(&arg).expect("should serialize");
    assert_eq!(json["name"], "optfield");
    assert!(json["description"].is_null());
    assert!(json["required"].is_null());
}

#[test]
fn mcp_prompt_message_image_content() {
    let msg = McpPromptMessage {
        role: "assistant".to_string(),
        content: McpPromptContent::Image {
            data: "imagedata".to_string(),
            mime_type: "image/gif".to_string(),
        },
    };

    assert_eq!(msg.role, "assistant");
    match &msg.content {
        McpPromptContent::Image { mime_type, .. } => {
            assert_eq!(mime_type, "image/gif");
        }
        _ => panic!("expected image content"),
    }
}

#[test]
fn mcp_prompt_message_resource_content() {
    let msg = McpPromptMessage {
        role: "user".to_string(),
        content: McpPromptContent::Resource {
            uri: "file:///readme.md".to_string(),
            mime_type: Some("text/markdown".to_string()),
            text: Some("# Title".to_string()),
        },
    };

    assert_eq!(msg.role, "user");
    match &msg.content {
        McpPromptContent::Resource { uri, .. } => {
            assert_eq!(uri, "file:///readme.md");
        }
        _ => panic!("expected resource content"),
    }
}

#[test]
fn mcp_prompt_result_empty() {
    let result = McpPromptResult {
        description: None,
        messages: vec![],
    };

    assert!(result.description.is_none());
    assert!(result.messages.is_empty());

    let json = serde_json::to_string(&result).expect("should serialize");
    let deserialized: McpPromptResult = serde_json::from_str(&json).expect("should deserialize");
    assert!(deserialized.messages.is_empty());
}

#[test]
fn mcp_prompt_result_multiple_messages() {
    let result = McpPromptResult {
        description: Some("Multi-turn prompt".to_string()),
        messages: vec![
            McpPromptMessage {
                role: "user".to_string(),
                content: McpPromptContent::Text("Question".to_string()),
            },
            McpPromptMessage {
                role: "assistant".to_string(),
                content: McpPromptContent::Text("Answer".to_string()),
            },
            McpPromptMessage {
                role: "user".to_string(),
                content: McpPromptContent::Text("Follow-up".to_string()),
            },
        ],
    };

    assert_eq!(result.messages.len(), 3);
    assert_eq!(result.messages[0].role, "user");
    assert_eq!(result.messages[1].role, "assistant");
    assert_eq!(result.messages[2].role, "user");
}

#[test]
fn mcp_prompt_serialization_roundtrip() {
    let prompt = McpPrompt {
        name: "roundtrip".to_string(),
        title: Some("Roundtrip Test".to_string()),
        description: Some("Tests serde roundtrip".to_string()),
        arguments: vec![McpPromptArgument {
            name: "input".to_string(),
            description: Some("The input".to_string()),
            required: Some(true),
        }],
    };

    let json = serde_json::to_string(&prompt).expect("should serialize");
    let deserialized: McpPrompt = serde_json::from_str(&json).expect("should deserialize");

    assert_eq!(deserialized.name, "roundtrip");
    assert_eq!(deserialized.title, Some("Roundtrip Test".to_string()));
    assert_eq!(deserialized.arguments.len(), 1);
    assert_eq!(deserialized.arguments[0].name, "input");
    assert_eq!(deserialized.arguments[0].required, Some(true));
}

#[test]
fn paginated_list_clone() {
    let list = PaginatedList {
        items: vec!["one".to_string(), "two".to_string()],
        next_cursor: Some("c".to_string()),
    };

    let cloned = list.clone();
    assert_eq!(cloned.items, list.items);
    assert_eq!(cloned.next_cursor, list.next_cursor);
}

#[test]
fn mcp_resource_clone() {
    let resource = McpResource {
        uri: "file:///a".to_string(),
        name: "a".to_string(),
        title: Some("A".to_string()),
        description: None,
        mime_type: None,
    };

    let cloned = resource.clone();
    assert_eq!(cloned.uri, resource.uri);
    assert_eq!(cloned.name, resource.name);
    assert_eq!(cloned.title, resource.title);
}

#[test]
fn mcp_resource_contents_clone() {
    let contents = McpResourceContents {
        uri: "file:///b".to_string(),
        mime_type: None,
        text: Some("content".to_string()),
        blob: None,
    };

    let cloned = contents.clone();
    assert_eq!(cloned.uri, contents.uri);
    assert_eq!(cloned.text, contents.text);
}

#[test]
fn mcp_prompt_clone() {
    let prompt = McpPrompt {
        name: "p".to_string(),
        title: None,
        description: None,
        arguments: vec![McpPromptArgument {
            name: "x".to_string(),
            description: None,
            required: None,
        }],
    };

    let cloned = prompt.clone();
    assert_eq!(cloned.name, prompt.name);
    assert_eq!(cloned.arguments.len(), 1);
}

#[test]
fn mcp_prompt_message_clone() {
    let msg = McpPromptMessage {
        role: "user".to_string(),
        content: McpPromptContent::Text("test".to_string()),
    };

    let cloned = msg.clone();
    assert_eq!(cloned.role, msg.role);
}

#[test]
fn mcp_prompt_result_clone() {
    let result = McpPromptResult {
        description: Some("desc".to_string()),
        messages: vec![],
    };

    let cloned = result.clone();
    assert_eq!(cloned.description, result.description);
}

#[test]
fn paginated_list_debug() {
    let list = PaginatedList {
        items: vec![42],
        next_cursor: None,
    };
    let debug = format!("{list:?}");
    assert!(debug.contains("42"));
}

#[test]
fn mcp_resource_debug() {
    let resource = McpResource {
        uri: "urn:debug".to_string(),
        name: "debug_res".to_string(),
        title: None,
        description: None,
        mime_type: None,
    };
    let debug = format!("{resource:?}");
    assert!(debug.contains("debug_res"));
}

#[test]
fn mcp_prompt_content_clone() {
    let text = McpPromptContent::Text("clone me".to_string());
    let cloned = text.clone();
    match cloned {
        McpPromptContent::Text(t) => assert_eq!(t, "clone me"),
        _ => panic!("expected text"),
    }

    let img = McpPromptContent::Image {
        data: "d".to_string(),
        mime_type: "image/png".to_string(),
    };
    let cloned_img = img.clone();
    match cloned_img {
        McpPromptContent::Image { data, mime_type } => {
            assert_eq!(data, "d");
            assert_eq!(mime_type, "image/png");
        }
        _ => panic!("expected image"),
    }

    let res = McpPromptContent::Resource {
        uri: "u".to_string(),
        mime_type: None,
        text: None,
    };
    let cloned_res = res.clone();
    match cloned_res {
        McpPromptContent::Resource { uri, .. } => assert_eq!(uri, "u"),
        _ => panic!("expected resource"),
    }
}
