use neuron_types::*;

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

// --- ToolOutput with is_error: true ---

#[test]
fn tool_output_error_flag() {
    let output = ToolOutput {
        content: vec![ContentItem::Text("permission denied".into())],
        structured_content: None,
        is_error: true,
    };
    let json = serde_json::to_string(&output).unwrap();
    let rt: ToolOutput = serde_json::from_str(&json).unwrap();
    assert!(rt.is_error);
    assert_eq!(rt.content.len(), 1);
}

// --- ToolOutput with structured_content ---

#[test]
fn tool_output_with_structured_content() {
    let output = ToolOutput {
        content: vec![ContentItem::Text("42".into())],
        structured_content: Some(serde_json::json!({"result": 42})),
        is_error: false,
    };
    let json = serde_json::to_string(&output).unwrap();
    let rt: ToolOutput = serde_json::from_str(&json).unwrap();
    assert!(!rt.is_error);
    assert_eq!(rt.structured_content.unwrap()["result"], 42);
}

// --- ToolOutput serde roundtrip with all fields ---

#[test]
fn tool_output_full_serde_roundtrip() {
    let output = ToolOutput {
        content: vec![
            ContentItem::Text("result text".into()),
            ContentItem::Image {
                source: ImageSource::Url {
                    url: "https://example.com/chart.png".into(),
                },
            },
        ],
        structured_content: Some(serde_json::json!({"data": [1, 2, 3]})),
        is_error: false,
    };
    let json = serde_json::to_string(&output).unwrap();
    let rt: ToolOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.content.len(), 2);
    assert!(!rt.is_error);
    assert_eq!(rt.structured_content.unwrap()["data"][1], 2);
}

// --- ToolDefinition with all optional fields populated ---

#[test]
fn tool_definition_all_fields_populated_serde() {
    let def = ToolDefinition {
        name: "web_search".into(),
        title: Some("Web Search Tool".into()),
        description: "Search the web for information".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["query"]
        }),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "results": { "type": "array" }
            }
        })),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(false),
            open_world_hint: Some(true),
        }),
        cache_control: Some(CacheControl {
            ttl: Some(CacheTtl::FiveMinutes),
        }),
    };
    let json = serde_json::to_string(&def).unwrap();
    let rt: ToolDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.name, "web_search");
    assert_eq!(rt.title.unwrap(), "Web Search Tool");
    assert!(rt.output_schema.is_some());
    let ann = rt.annotations.unwrap();
    assert_eq!(ann.read_only_hint, Some(true));
    assert_eq!(ann.destructive_hint, Some(false));
    assert_eq!(ann.idempotent_hint, Some(false));
    assert_eq!(ann.open_world_hint, Some(true));
    assert!(rt.cache_control.is_some());
}

// --- ToolAnnotations with all None fields ---

#[test]
fn tool_annotations_all_none_serde() {
    let ann = ToolAnnotations {
        read_only_hint: None,
        destructive_hint: None,
        idempotent_hint: None,
        open_world_hint: None,
    };
    let json = serde_json::to_string(&ann).unwrap();
    let rt: ToolAnnotations = serde_json::from_str(&json).unwrap();
    assert!(rt.read_only_hint.is_none());
    assert!(rt.destructive_hint.is_none());
    assert!(rt.idempotent_hint.is_none());
    assert!(rt.open_world_hint.is_none());
}

// --- ToolContext with populated environment ---

#[test]
fn tool_context_with_environment() {
    let mut env = std::collections::HashMap::new();
    env.insert("API_KEY".into(), "secret123".into());
    env.insert("REGION".into(), "us-east-1".into());

    let ctx = ToolContext {
        cwd: std::path::PathBuf::from("/home/user/project"),
        session_id: "sess-42".into(),
        environment: env,
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    };
    assert_eq!(ctx.session_id, "sess-42");
    assert_eq!(ctx.environment.len(), 2);
    assert_eq!(ctx.environment["API_KEY"], "secret123");
    assert_eq!(ctx.cwd.to_str().unwrap(), "/home/user/project");
}

// --- ToolContext default has a valid cwd ---

#[test]
fn tool_context_default_has_valid_cwd() {
    let ctx = ToolContext::default();
    // The default cwd should be either the current directory or /tmp
    assert!(ctx.cwd.exists() || ctx.cwd == std::path::Path::new("/tmp"));
}

// --- ToolOutput with empty content ---

#[test]
fn tool_output_empty_content_serde() {
    let output = ToolOutput {
        content: vec![],
        structured_content: None,
        is_error: false,
    };
    let json = serde_json::to_string(&output).unwrap();
    let rt: ToolOutput = serde_json::from_str(&json).unwrap();
    assert!(rt.content.is_empty());
    assert!(!rt.is_error);
}
