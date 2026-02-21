use agent_types::*;

#[test]
fn tool_definition_serde() {
    let def = ToolDefinition {
        name: "read_file".into(),
        title: None,
        description: "Read a file".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        }),
        output_schema: None,
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: None,
        }),
        cache_control: None,
    };
    let json = serde_json::to_string(&def).unwrap();
    let rt: ToolDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.name, "read_file");
    assert!(rt.annotations.unwrap().read_only_hint.unwrap());
}

#[test]
fn tool_output_text() {
    let output = ToolOutput {
        content: vec![ContentItem::Text("file contents".into())],
        structured_content: None,
        is_error: false,
    };
    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("file contents"));
}

#[test]
fn tool_context_creation() {
    let ctx = ToolContext {
        cwd: std::path::PathBuf::from("/tmp"),
        session_id: "test-session".into(),
        environment: std::collections::HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    };
    assert_eq!(ctx.session_id, "test-session");
}
