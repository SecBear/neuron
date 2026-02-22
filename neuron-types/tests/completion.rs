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
