//! Integration tests for agent-mcp.
//!
//! Note: Tests that require a running MCP server are ignored by default.
//! Run them with `cargo test -- --ignored` when a server is available.

use agent_mcp::*;

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
