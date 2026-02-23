use neuron_types::*;

#[test]
fn completion_request_minimal() {
    let req = CompletionRequest {
        model: "claude-sonnet-4-20250514".into(),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("hi".into())],
        }],
        system: None,
        tools: vec![],
        max_tokens: Some(1024),
        temperature: None,
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
        context_management: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("claude-sonnet"));
}

#[test]
fn system_prompt_blocks_with_cache() {
    let sys = SystemPrompt::Blocks(vec![SystemBlock {
        text: "You are helpful.".into(),
        cache_control: Some(CacheControl {
            ttl: Some(CacheTtl::OneHour),
        }),
    }]);
    let json = serde_json::to_string(&sys).unwrap();
    let rt: SystemPrompt = serde_json::from_str(&json).unwrap();
    if let SystemPrompt::Blocks(blocks) = rt {
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].cache_control.is_some());
    } else {
        panic!("expected Blocks");
    }
}

#[test]
fn completion_response_serde() {
    let resp = CompletionResponse {
        id: "msg_123".into(),
        model: "claude-sonnet-4-20250514".into(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("Paris".into())],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: None,
            cache_creation_tokens: None,
            reasoning_tokens: None,
            iterations: None,
        },
        stop_reason: StopReason::EndTurn,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let rt: CompletionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.id, "msg_123");
}

#[test]
fn token_usage_default() {
    let usage = TokenUsage::default();
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 0);
}

#[test]
fn system_prompt_from_str() {
    let prompt: SystemPrompt = "You are helpful.".into();
    assert!(matches!(prompt, SystemPrompt::Text(s) if s == "You are helpful."));
}

#[test]
fn system_prompt_from_string() {
    let prompt: SystemPrompt = String::from("You are helpful.").into();
    assert!(matches!(prompt, SystemPrompt::Text(s) if s == "You are helpful."));
}

#[test]
fn message_user_constructor() {
    let msg = Message::user("Hello!");
    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content.len(), 1);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello!"));
}

#[test]
fn message_assistant_constructor() {
    let msg = Message::assistant("Hi there");
    assert_eq!(msg.role, Role::Assistant);
    assert_eq!(msg.content.len(), 1);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hi there"));
}

#[test]
fn message_system_constructor() {
    let msg = Message::system("You are helpful");
    assert_eq!(msg.role, Role::System);
    assert_eq!(msg.content.len(), 1);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "You are helpful"));
}

#[test]
fn tool_context_default() {
    let ctx = ToolContext::default();
    assert_eq!(ctx.session_id, "");
    assert!(ctx.environment.is_empty());
    assert!(ctx.progress_reporter.is_none());
}

// --- CompletionRequest Default ---

#[test]
fn completion_request_default() {
    let req = CompletionRequest::default();
    assert!(req.model.is_empty());
    assert!(req.messages.is_empty());
    assert!(req.system.is_none());
    assert!(req.tools.is_empty());
    assert_eq!(req.max_tokens, None);
    assert_eq!(req.temperature, None);
    assert_eq!(req.top_p, None);
    assert!(req.stop_sequences.is_empty());
    assert!(req.tool_choice.is_none());
    assert!(req.response_format.is_none());
    assert!(req.thinking.is_none());
    assert!(req.reasoning_effort.is_none());
    assert!(req.extra.is_none());
    assert!(req.context_management.is_none());
}

// --- CompletionRequest with ..Default::default() pattern ---

#[test]
fn completion_request_partial_with_default() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        messages: vec![Message::user("hi")],
        max_tokens: Some(2048),
        ..Default::default()
    };
    assert_eq!(req.model, "gpt-4");
    assert_eq!(req.messages.len(), 1);
    assert_eq!(req.max_tokens, Some(2048));
    assert!(req.tools.is_empty());
}

// --- TokenUsage with all optional fields populated ---

#[test]
fn token_usage_with_all_fields() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: Some(20),
        cache_creation_tokens: Some(80),
        reasoning_tokens: Some(30),
        iterations: Some(vec![UsageIteration {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: Some(20),
            cache_creation_tokens: Some(80),
        }]),
    };
    let json = serde_json::to_string(&usage).unwrap();
    let rt: TokenUsage = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.input_tokens, 100);
    assert_eq!(rt.output_tokens, 50);
    assert_eq!(rt.cache_read_tokens, Some(20));
    assert_eq!(rt.cache_creation_tokens, Some(80));
    assert_eq!(rt.reasoning_tokens, Some(30));
    assert_eq!(rt.iterations.as_ref().unwrap().len(), 1);
}

#[test]
fn token_usage_default_optional_fields_are_none() {
    let usage = TokenUsage::default();
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 0);
    assert!(usage.cache_read_tokens.is_none());
    assert!(usage.cache_creation_tokens.is_none());
    assert!(usage.reasoning_tokens.is_none());
    assert!(usage.iterations.is_none());
}

// --- UsageIteration ---

#[test]
fn usage_iteration_default() {
    let iter = UsageIteration::default();
    assert_eq!(iter.input_tokens, 0);
    assert_eq!(iter.output_tokens, 0);
    assert!(iter.cache_read_tokens.is_none());
    assert!(iter.cache_creation_tokens.is_none());
}

#[test]
fn usage_iteration_serde_roundtrip() {
    let iter = UsageIteration {
        input_tokens: 500,
        output_tokens: 200,
        cache_read_tokens: Some(100),
        cache_creation_tokens: Some(400),
    };
    let json = serde_json::to_string(&iter).unwrap();
    let rt: UsageIteration = serde_json::from_str(&json).unwrap();
    assert_eq!(iter, rt);
}

// --- ToolChoice serde ---

#[test]
fn tool_choice_auto_serde() {
    let tc = ToolChoice::Auto;
    let json = serde_json::to_string(&tc).unwrap();
    let rt: ToolChoice = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, ToolChoice::Auto));
}

#[test]
fn tool_choice_none_serde() {
    let tc = ToolChoice::None;
    let json = serde_json::to_string(&tc).unwrap();
    let rt: ToolChoice = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, ToolChoice::None));
}

#[test]
fn tool_choice_required_serde() {
    let tc = ToolChoice::Required;
    let json = serde_json::to_string(&tc).unwrap();
    let rt: ToolChoice = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, ToolChoice::Required));
}

#[test]
fn tool_choice_specific_serde() {
    let tc = ToolChoice::Specific {
        name: "read_file".into(),
    };
    let json = serde_json::to_string(&tc).unwrap();
    let rt: ToolChoice = serde_json::from_str(&json).unwrap();
    if let ToolChoice::Specific { name } = rt {
        assert_eq!(name, "read_file");
    } else {
        panic!("expected Specific");
    }
}

// --- ResponseFormat serde ---

#[test]
fn response_format_text_serde() {
    let rf = ResponseFormat::Text;
    let json = serde_json::to_string(&rf).unwrap();
    let rt: ResponseFormat = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, ResponseFormat::Text));
}

#[test]
fn response_format_json_object_serde() {
    let rf = ResponseFormat::JsonObject;
    let json = serde_json::to_string(&rf).unwrap();
    let rt: ResponseFormat = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, ResponseFormat::JsonObject));
}

#[test]
fn response_format_json_schema_serde() {
    let rf = ResponseFormat::JsonSchema {
        name: "person".into(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name"]
        }),
        strict: true,
    };
    let json = serde_json::to_string(&rf).unwrap();
    let rt: ResponseFormat = serde_json::from_str(&json).unwrap();
    if let ResponseFormat::JsonSchema {
        name,
        schema,
        strict,
    } = rt
    {
        assert_eq!(name, "person");
        assert!(strict);
        assert_eq!(schema["properties"]["name"]["type"], "string");
    } else {
        panic!("expected JsonSchema");
    }
}

// --- ThinkingConfig serde ---

#[test]
fn thinking_config_enabled_serde() {
    let tc = ThinkingConfig::Enabled {
        budget_tokens: 10000,
    };
    let json = serde_json::to_string(&tc).unwrap();
    let rt: ThinkingConfig = serde_json::from_str(&json).unwrap();
    if let ThinkingConfig::Enabled { budget_tokens } = rt {
        assert_eq!(budget_tokens, 10000);
    } else {
        panic!("expected Enabled");
    }
}

#[test]
fn thinking_config_disabled_serde() {
    let tc = ThinkingConfig::Disabled;
    let json = serde_json::to_string(&tc).unwrap();
    let rt: ThinkingConfig = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, ThinkingConfig::Disabled));
}

#[test]
fn thinking_config_adaptive_serde() {
    let tc = ThinkingConfig::Adaptive;
    let json = serde_json::to_string(&tc).unwrap();
    let rt: ThinkingConfig = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, ThinkingConfig::Adaptive));
}

// --- ReasoningEffort serde ---

#[test]
fn reasoning_effort_all_variants_serde() {
    for effort in [
        ReasoningEffort::None,
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
    ] {
        let json = serde_json::to_string(&effort).unwrap();
        let rt: ReasoningEffort = serde_json::from_str(&json).unwrap();
        // Use Debug representation for comparison since PartialEq is not derived
        assert_eq!(format!("{rt:?}"), format!("{effort:?}"));
    }
}

// --- ContextManagement and ContextEdit ---

#[test]
fn context_management_default() {
    let cm = ContextManagement::default();
    assert!(cm.edits.is_empty());
}

#[test]
fn context_management_with_compact_edit_serde() {
    let cm = ContextManagement {
        edits: vec![ContextEdit::Compact {
            strategy: "compact_20260112".into(),
        }],
    };
    let json = serde_json::to_string(&cm).unwrap();
    let rt: ContextManagement = serde_json::from_str(&json).unwrap();
    assert_eq!(cm, rt);
    assert_eq!(rt.edits.len(), 1);
    let ContextEdit::Compact { strategy } = &rt.edits[0];
    assert_eq!(strategy, "compact_20260112");
}

// --- StopReason serde ---

#[test]
fn stop_reason_all_variants_serde() {
    let variants = vec![
        StopReason::EndTurn,
        StopReason::ToolUse,
        StopReason::MaxTokens,
        StopReason::StopSequence,
        StopReason::ContentFilter,
        StopReason::Compaction,
    ];
    for sr in variants {
        let json = serde_json::to_string(&sr).unwrap();
        let rt: StopReason = serde_json::from_str(&json).unwrap();
        assert_eq!(sr, rt);
    }
}

// --- CacheTtl FiveMinutes ---

#[test]
fn cache_ttl_five_minutes_serde() {
    let cc = CacheControl {
        ttl: Some(CacheTtl::FiveMinutes),
    };
    let json = serde_json::to_string(&cc).unwrap();
    let rt: CacheControl = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt.ttl, Some(CacheTtl::FiveMinutes)));
}

#[test]
fn cache_control_no_ttl_serde() {
    let cc = CacheControl { ttl: None };
    let json = serde_json::to_string(&cc).unwrap();
    let rt: CacheControl = serde_json::from_str(&json).unwrap();
    assert!(rt.ttl.is_none());
}

// --- SystemPrompt::Text serde ---

#[test]
fn system_prompt_text_serde() {
    let sp = SystemPrompt::Text("You are a coding assistant.".into());
    let json = serde_json::to_string(&sp).unwrap();
    let rt: SystemPrompt = serde_json::from_str(&json).unwrap();
    if let SystemPrompt::Text(text) = rt {
        assert_eq!(text, "You are a coding assistant.");
    } else {
        panic!("expected Text");
    }
}

// --- SystemBlock without cache control ---

#[test]
fn system_block_no_cache_serde() {
    let block = SystemBlock {
        text: "Block text".into(),
        cache_control: None,
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: SystemBlock = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.text, "Block text");
    assert!(rt.cache_control.is_none());
}

// --- CompletionRequest with all fields populated ---

#[test]
fn completion_request_fully_populated_serde() {
    let req = CompletionRequest {
        model: "claude-opus-4-20250514".into(),
        messages: vec![Message::user("hi")],
        system: Some(SystemPrompt::Text("Be helpful.".into())),
        tools: vec![ToolDefinition {
            name: "search".into(),
            title: Some("Web Search".into()),
            description: "Search the web".into(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }],
        max_tokens: Some(4096),
        temperature: Some(0.7),
        top_p: Some(0.9),
        stop_sequences: vec!["STOP".into()],
        tool_choice: Some(ToolChoice::Auto),
        response_format: Some(ResponseFormat::Text),
        thinking: Some(ThinkingConfig::Enabled {
            budget_tokens: 5000,
        }),
        reasoning_effort: Some(ReasoningEffort::High),
        extra: Some(serde_json::json!({"custom_field": true})),
        context_management: Some(ContextManagement {
            edits: vec![ContextEdit::Compact {
                strategy: "default".into(),
            }],
        }),
    };
    let json = serde_json::to_string(&req).unwrap();
    let rt: CompletionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.model, "claude-opus-4-20250514");
    assert_eq!(rt.messages.len(), 1);
    assert!(rt.system.is_some());
    assert_eq!(rt.tools.len(), 1);
    assert_eq!(rt.max_tokens, Some(4096));
    assert_eq!(rt.temperature, Some(0.7));
    assert_eq!(rt.top_p, Some(0.9));
    assert_eq!(rt.stop_sequences, vec!["STOP"]);
    assert!(rt.tool_choice.is_some());
    assert!(rt.response_format.is_some());
    assert!(rt.thinking.is_some());
    assert!(rt.reasoning_effort.is_some());
    assert!(rt.extra.is_some());
    assert!(rt.context_management.is_some());
}

// --- CompletionResponse with all stop reasons ---

#[test]
fn completion_response_tool_use_stop_serde() {
    let resp = CompletionResponse {
        id: "msg_456".into(),
        model: "test-model".into(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "t1".into(),
                name: "read_file".into(),
                input: serde_json::json!({"path": "/tmp/f"}),
            }],
        },
        usage: TokenUsage {
            input_tokens: 50,
            output_tokens: 20,
            ..Default::default()
        },
        stop_reason: StopReason::ToolUse,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let rt: CompletionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.stop_reason, StopReason::ToolUse);
}

#[test]
fn completion_response_compaction_stop_serde() {
    let resp = CompletionResponse {
        id: "msg_789".into(),
        model: "test-model".into(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Compaction {
                content: "Summarized context".into(),
            }],
        },
        usage: TokenUsage {
            input_tokens: 1000,
            output_tokens: 100,
            ..Default::default()
        },
        stop_reason: StopReason::Compaction,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let rt: CompletionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.stop_reason, StopReason::Compaction);
}

// --- EmbeddingRequest with dimensions ---

#[test]
fn embedding_request_with_dimensions() {
    let req = EmbeddingRequest {
        model: "text-embedding-3-small".into(),
        input: vec!["hello".into(), "world".into()],
        dimensions: Some(256),
        ..Default::default()
    };
    assert_eq!(req.model, "text-embedding-3-small");
    assert_eq!(req.input.len(), 2);
    assert_eq!(req.dimensions, Some(256));
}

// --- EmbeddingRequest with extra fields ---

#[test]
fn embedding_request_with_extra() {
    let mut extra = std::collections::HashMap::new();
    extra.insert("encoding_format".into(), serde_json::json!("float"));
    let req = EmbeddingRequest {
        model: "text-embedding-3-small".into(),
        input: vec!["test".into()],
        extra,
        ..Default::default()
    };
    assert_eq!(req.extra["encoding_format"], "float");
}
