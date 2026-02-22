//! Request/response mapping between neuron-types and the Anthropic Messages API format.
//!
//! Reference: <https://docs.anthropic.com/en/api/messages>

use neuron_types::{
    CacheTtl, CompletionRequest, CompletionResponse, ContentBlock, ContentItem, DocumentSource,
    ImageSource, Message, ProviderError, Role, StopReason, SystemPrompt, ThinkingConfig,
    TokenUsage, ToolChoice, ToolDefinition,
};

// ─── Request mapping ─────────────────────────────────────────────────────────

/// Convert a [`CompletionRequest`] into the Anthropic Messages API JSON body.
///
/// The returned value does **not** include `"stream"` — callers add that key.
#[must_use]
pub fn to_api_request(req: &CompletionRequest, default_model: &str) -> serde_json::Value {
    let model = if req.model.is_empty() {
        default_model.to_string()
    } else {
        req.model.clone()
    };

    let mut body = serde_json::json!({
        "model": model,
        "messages": map_messages(&req.messages),
        "max_tokens": req.max_tokens.unwrap_or(4096),
    });

    // System prompt
    if let Some(system) = &req.system {
        body["system"] = map_system_prompt(system);
    }

    // Temperature
    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::Value::from(temp);
    }

    // Top-p
    if let Some(top_p) = req.top_p {
        body["top_p"] = serde_json::Value::from(top_p);
    }

    // Stop sequences
    if !req.stop_sequences.is_empty() {
        body["stop_sequences"] = serde_json::Value::Array(
            req.stop_sequences
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        );
    }

    // Tools
    if !req.tools.is_empty() {
        body["tools"] = serde_json::Value::Array(
            req.tools.iter().map(map_tool_definition).collect(),
        );
    }

    // Tool choice
    if let Some(choice) = &req.tool_choice {
        body["tool_choice"] = map_tool_choice(choice);
    }

    // Thinking / extended thinking
    if let Some(thinking) = &req.thinking
        && let Some(thinking_val) = map_thinking_config(thinking)
    {
        body["thinking"] = thinking_val;
    }

    // Context management (server-side compaction)
    if let Some(ctx_mgmt) = &req.context_management {
        let edits: Vec<serde_json::Value> = ctx_mgmt
            .edits
            .iter()
            .map(|edit| match edit {
                neuron_types::ContextEdit::Compact { strategy } => serde_json::json!({
                    "type": "auto",
                    "trigger": strategy,
                }),
            })
            .collect();
        body["context_management"] = serde_json::json!({ "edits": edits });
    }

    // Merge extra provider-specific fields last (they can override anything above)
    if let Some(serde_json::Value::Object(extra_map)) = &req.extra
        && let serde_json::Value::Object(body_map) = &mut body
    {
        for (k, v) in extra_map {
            body_map.insert(k.clone(), v.clone());
        }
    }

    body
}

/// Map a list of [`Message`]s to Anthropic's message array format.
fn map_messages(messages: &[Message]) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = messages
        .iter()
        .filter_map(|msg| {
            // Anthropic does not accept system messages inline — they go in the top-level
            // "system" field. Filter them out here.
            if msg.role == Role::System {
                return None;
            }
            let role_str = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => unreachable!("filtered above"),
            };
            let content = map_content_blocks(&msg.content);
            Some(serde_json::json!({ "role": role_str, "content": content }))
        })
        .collect();
    serde_json::Value::Array(arr)
}

/// Map a slice of [`ContentBlock`]s to Anthropic's content array.
pub(crate) fn map_content_blocks(blocks: &[ContentBlock]) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = blocks.iter().map(map_content_block).collect();
    serde_json::Value::Array(arr)
}

/// Map a single [`ContentBlock`] to its Anthropic JSON representation.
pub(crate) fn map_content_block(block: &ContentBlock) -> serde_json::Value {
    match block {
        ContentBlock::Text(text) => serde_json::json!({
            "type": "text",
            "text": text,
        }),
        ContentBlock::Thinking { thinking, signature } => serde_json::json!({
            "type": "thinking",
            "thinking": thinking,
            "signature": signature,
        }),
        ContentBlock::RedactedThinking { data } => serde_json::json!({
            "type": "redacted_thinking",
            "data": data,
        }),
        ContentBlock::ToolUse { id, name, input } => serde_json::json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input,
        }),
        ContentBlock::ToolResult { tool_use_id, content, is_error } => {
            let mapped_content: Vec<serde_json::Value> = content
                .iter()
                .map(map_content_item)
                .collect();
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": mapped_content,
                "is_error": is_error,
            })
        }
        ContentBlock::Image { source } => serde_json::json!({
            "type": "image",
            "source": map_image_source(source),
        }),
        ContentBlock::Document { source } => serde_json::json!({
            "type": "document",
            "source": map_document_source(source),
        }),
        ContentBlock::Compaction { content } => serde_json::json!({
            "type": "compaction",
            "content": content,
        }),
    }
}

/// Map a [`ContentItem`] to its Anthropic JSON representation.
fn map_content_item(item: &ContentItem) -> serde_json::Value {
    match item {
        ContentItem::Text(text) => serde_json::json!({ "type": "text", "text": text }),
        ContentItem::Image { source } => serde_json::json!({
            "type": "image",
            "source": map_image_source(source),
        }),
    }
}

/// Map an [`ImageSource`] to Anthropic's image source format.
fn map_image_source(source: &ImageSource) -> serde_json::Value {
    match source {
        ImageSource::Base64 { media_type, data } => serde_json::json!({
            "type": "base64",
            "media_type": media_type,
            "data": data,
        }),
        ImageSource::Url { url } => serde_json::json!({
            "type": "url",
            "url": url,
        }),
    }
}

/// Map a [`DocumentSource`] to Anthropic's document source format.
fn map_document_source(source: &DocumentSource) -> serde_json::Value {
    match source {
        DocumentSource::Base64Pdf { data } => serde_json::json!({
            "type": "base64",
            "media_type": "application/pdf",
            "data": data,
        }),
        DocumentSource::PlainText { data } => serde_json::json!({
            "type": "text",
            "data": data,
        }),
        DocumentSource::Url { url } => serde_json::json!({
            "type": "url",
            "url": url,
        }),
    }
}

/// Map a [`SystemPrompt`] to Anthropic's system field value.
fn map_system_prompt(system: &SystemPrompt) -> serde_json::Value {
    match system {
        SystemPrompt::Text(text) => serde_json::Value::String(text.clone()),
        SystemPrompt::Blocks(blocks) => {
            let arr: Vec<serde_json::Value> = blocks
                .iter()
                .map(|block| {
                    let mut obj = serde_json::json!({
                        "type": "text",
                        "text": block.text,
                    });
                    if let Some(cache_control) = &block.cache_control {
                        obj["cache_control"] = map_cache_control(cache_control);
                    }
                    obj
                })
                .collect();
            serde_json::Value::Array(arr)
        }
    }
}

/// Map a [`neuron_types::CacheControl`] to Anthropic's cache_control object.
fn map_cache_control(cc: &neuron_types::CacheControl) -> serde_json::Value {
    let ttl = match cc.ttl {
        Some(CacheTtl::FiveMinutes) => "ephemeral",
        // Anthropic currently only supports "ephemeral" (5 min); 1-hour maps to it too
        Some(CacheTtl::OneHour) => "ephemeral",
        None => "ephemeral",
    };
    serde_json::json!({ "type": ttl })
}

/// Map a [`ToolDefinition`] to Anthropic's tool definition format.
fn map_tool_definition(tool: &ToolDefinition) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    });
    if let Some(cache_control) = &tool.cache_control {
        obj["cache_control"] = map_cache_control(cache_control);
    }
    obj
}

/// Map a [`ToolChoice`] to Anthropic's tool_choice format.
fn map_tool_choice(choice: &ToolChoice) -> serde_json::Value {
    match choice {
        ToolChoice::Auto => serde_json::json!({ "type": "auto" }),
        ToolChoice::None => serde_json::json!({ "type": "none" }),
        ToolChoice::Required => serde_json::json!({ "type": "any" }),
        ToolChoice::Specific { name } => serde_json::json!({ "type": "tool", "name": name }),
    }
}

/// Map a [`ThinkingConfig`] to Anthropic's thinking object, or `None` if disabled.
fn map_thinking_config(config: &ThinkingConfig) -> Option<serde_json::Value> {
    match config {
        ThinkingConfig::Enabled { budget_tokens } => Some(serde_json::json!({
            "type": "enabled",
            "budget_tokens": budget_tokens,
        })),
        ThinkingConfig::Disabled => Some(serde_json::json!({ "type": "disabled" })),
        ThinkingConfig::Adaptive => None,
    }
}

// ─── Response mapping ─────────────────────────────────────────────────────────

/// Parse an Anthropic Messages API response JSON into a [`CompletionResponse`].
///
/// # Errors
///
/// Returns [`ProviderError::InvalidRequest`] if required fields are missing or malformed.
pub fn from_api_response(body: &serde_json::Value) -> Result<CompletionResponse, ProviderError> {
    let id = body["id"]
        .as_str()
        .ok_or_else(|| ProviderError::InvalidRequest("missing 'id' in response".into()))?
        .to_string();

    let model = body["model"]
        .as_str()
        .ok_or_else(|| ProviderError::InvalidRequest("missing 'model' in response".into()))?
        .to_string();

    let content_arr = body["content"]
        .as_array()
        .ok_or_else(|| ProviderError::InvalidRequest("missing 'content' array in response".into()))?;

    let mut content_blocks = Vec::with_capacity(content_arr.len());
    for block in content_arr {
        content_blocks.push(parse_content_block(block)?);
    }

    let usage = parse_usage(&body["usage"]);

    let stop_reason = body["stop_reason"]
        .as_str()
        .map(parse_stop_reason)
        .unwrap_or(StopReason::EndTurn);

    Ok(CompletionResponse {
        id,
        model,
        message: Message {
            role: Role::Assistant,
            content: content_blocks,
        },
        usage,
        stop_reason,
    })
}

/// Parse a single content block from the Anthropic response JSON.
fn parse_content_block(block: &serde_json::Value) -> Result<ContentBlock, ProviderError> {
    let block_type = block["type"]
        .as_str()
        .ok_or_else(|| ProviderError::InvalidRequest("content block missing 'type'".into()))?;

    match block_type {
        "text" => {
            let text = block["text"]
                .as_str()
                .ok_or_else(|| ProviderError::InvalidRequest("text block missing 'text'".into()))?
                .to_string();
            Ok(ContentBlock::Text(text))
        }
        "thinking" => {
            let thinking = block["thinking"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let signature = block["signature"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            Ok(ContentBlock::Thinking { thinking, signature })
        }
        "redacted_thinking" => {
            let data = block["data"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            Ok(ContentBlock::RedactedThinking { data })
        }
        "tool_use" => {
            let id = block["id"]
                .as_str()
                .ok_or_else(|| ProviderError::InvalidRequest("tool_use block missing 'id'".into()))?
                .to_string();
            let name = block["name"]
                .as_str()
                .ok_or_else(|| ProviderError::InvalidRequest("tool_use block missing 'name'".into()))?
                .to_string();
            let input = block["input"].clone();
            Ok(ContentBlock::ToolUse { id, name, input })
        }
        "compaction" => {
            let content = block["content"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            Ok(ContentBlock::Compaction { content })
        }
        other => Err(ProviderError::InvalidRequest(format!(
            "unknown content block type: {other}"
        ))),
    }
}

/// Parse [`TokenUsage`] from the Anthropic response `usage` field.
fn parse_usage(usage: &serde_json::Value) -> TokenUsage {
    let iterations = usage["iterations"].as_array().map(|arr| {
        arr.iter()
            .map(|iter_val| neuron_types::UsageIteration {
                input_tokens: iter_val["input_tokens"].as_u64().unwrap_or(0) as usize,
                output_tokens: iter_val["output_tokens"].as_u64().unwrap_or(0) as usize,
                cache_read_tokens: iter_val["cache_read_input_tokens"]
                    .as_u64()
                    .map(|n| n as usize),
                cache_creation_tokens: iter_val["cache_creation_input_tokens"]
                    .as_u64()
                    .map(|n| n as usize),
            })
            .collect()
    });

    TokenUsage {
        input_tokens: usage["input_tokens"].as_u64().unwrap_or(0) as usize,
        output_tokens: usage["output_tokens"].as_u64().unwrap_or(0) as usize,
        cache_read_tokens: usage["cache_read_input_tokens"].as_u64().map(|n| n as usize),
        cache_creation_tokens: usage["cache_creation_input_tokens"]
            .as_u64()
            .map(|n| n as usize),
        reasoning_tokens: None,
        iterations,
    }
}

/// Map an Anthropic `stop_reason` string to a [`StopReason`].
fn parse_stop_reason(reason: &str) -> StopReason {
    match reason {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        "compaction" => StopReason::Compaction,
        _ => StopReason::EndTurn,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use neuron_types::{
        CacheControl, ContentBlock, Message, Role, SystemBlock, SystemPrompt, ToolDefinition,
    };

    use super::*;

    fn minimal_request() -> CompletionRequest {
        CompletionRequest {
            model: String::new(),
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text("Hello".into())],
            }],
            system: None,
            tools: vec![],
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop_sequences: vec![],
            tool_choice: None,
            response_format: None,
            thinking: None,
            reasoning_effort: None,
            extra: None,
            context_management: None,
        }
    }

    #[test]
    fn minimal_request_uses_default_model() {
        let req = minimal_request();
        let body = to_api_request(&req, "claude-test-model");
        assert_eq!(body["model"], "claude-test-model");
    }

    #[test]
    fn explicit_model_takes_precedence() {
        let mut req = minimal_request();
        req.model = "claude-opus-4-5".into();
        let body = to_api_request(&req, "default-model");
        assert_eq!(body["model"], "claude-opus-4-5");
    }

    #[test]
    fn messages_mapped_correctly() {
        let req = minimal_request();
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        let content = messages[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Hello");
    }

    #[test]
    fn system_text_prompt_mapped_as_string() {
        let mut req = minimal_request();
        req.system = Some(SystemPrompt::Text("You are a helpful assistant.".into()));
        let body = to_api_request(&req, "m");
        assert_eq!(body["system"], "You are a helpful assistant.");
    }

    #[test]
    fn system_blocks_prompt_mapped_as_array() {
        let mut req = minimal_request();
        req.system = Some(SystemPrompt::Blocks(vec![SystemBlock {
            text: "Be concise.".into(),
            cache_control: Some(CacheControl { ttl: None }),
        }]));
        let body = to_api_request(&req, "m");
        let system = body["system"].as_array().unwrap();
        assert_eq!(system.len(), 1);
        assert_eq!(system[0]["type"], "text");
        assert_eq!(system[0]["text"], "Be concise.");
        assert_eq!(system[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn tool_definition_mapped_correctly() {
        let mut req = minimal_request();
        req.tools = vec![ToolDefinition {
            name: "search".into(),
            title: None,
            description: "Search the web".into(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }];
        let body = to_api_request(&req, "m");
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "search");
        assert_eq!(tools[0]["description"], "Search the web");
        assert!(tools[0]["input_schema"].is_object());
    }

    #[test]
    fn tool_choice_auto_maps_correctly() {
        let mut req = minimal_request();
        req.tool_choice = Some(ToolChoice::Auto);
        let body = to_api_request(&req, "m");
        assert_eq!(body["tool_choice"]["type"], "auto");
    }

    #[test]
    fn tool_choice_none_maps_correctly() {
        let mut req = minimal_request();
        req.tool_choice = Some(ToolChoice::None);
        let body = to_api_request(&req, "m");
        assert_eq!(body["tool_choice"]["type"], "none");
    }

    #[test]
    fn tool_choice_required_maps_to_any() {
        let mut req = minimal_request();
        req.tool_choice = Some(ToolChoice::Required);
        let body = to_api_request(&req, "m");
        assert_eq!(body["tool_choice"]["type"], "any");
    }

    #[test]
    fn tool_choice_specific_maps_correctly() {
        let mut req = minimal_request();
        req.tool_choice = Some(ToolChoice::Specific { name: "search".into() });
        let body = to_api_request(&req, "m");
        assert_eq!(body["tool_choice"]["type"], "tool");
        assert_eq!(body["tool_choice"]["name"], "search");
    }

    #[test]
    fn parse_response_text_only() {
        let body = serde_json::json!({
            "id": "msg_01XFDUDYJgAACzvnptvVoYEL",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Hello!" }],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
            }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.id, "msg_01XFDUDYJgAACzvnptvVoYEL");
        assert_eq!(resp.model, "claude-sonnet-4-20250514");
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert!(matches!(&resp.message.content[0], ContentBlock::Text(t) if t == "Hello!"));
    }

    #[test]
    fn parse_response_tool_use() {
        let body = serde_json::json!({
            "id": "msg_abc",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{
                "type": "tool_use",
                "id": "toolu_01",
                "name": "search",
                "input": { "query": "rust" }
            }],
            "stop_reason": "tool_use",
            "usage": { "input_tokens": 20, "output_tokens": 15 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::ToolUse);
        assert!(matches!(
            &resp.message.content[0],
            ContentBlock::ToolUse { name, .. } if name == "search"
        ));
    }

    #[test]
    fn parse_response_cache_tokens() {
        let body = serde_json::json!({
            "id": "msg_cached",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Hi" }],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 5,
                "output_tokens": 2,
                "cache_read_input_tokens": 1000,
                "cache_creation_input_tokens": 500,
            }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.usage.cache_read_tokens, Some(1000));
        assert_eq!(resp.usage.cache_creation_tokens, Some(500));
    }

    #[test]
    fn system_messages_are_filtered_from_messages_array() {
        let mut req = minimal_request();
        req.messages.push(Message {
            role: Role::System,
            content: vec![ContentBlock::Text("System".into())],
        });
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        // System message should be filtered out
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn tool_use_content_block_maps_correctly() {
        let block = ContentBlock::ToolUse {
            id: "toolu_01".into(),
            name: "search".into(),
            input: serde_json::json!({ "q": "test" }),
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "tool_use");
        assert_eq!(val["id"], "toolu_01");
        assert_eq!(val["name"], "search");
        assert_eq!(val["input"]["q"], "test");
    }

    #[test]
    fn tool_result_content_block_maps_correctly() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "toolu_01".into(),
            content: vec![ContentItem::Text("result".into())],
            is_error: false,
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "tool_result");
        assert_eq!(val["tool_use_id"], "toolu_01");
        assert_eq!(val["is_error"], false);
    }

    #[test]
    fn thinking_block_maps_correctly() {
        let block = ContentBlock::Thinking {
            thinking: "I am thinking...".into(),
            signature: "sig123".into(),
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "thinking");
        assert_eq!(val["thinking"], "I am thinking...");
        assert_eq!(val["signature"], "sig123");
    }
}
