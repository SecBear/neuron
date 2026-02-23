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
        body["tools"] =
            serde_json::Value::Array(req.tools.iter().map(map_tool_definition).collect());
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
        ContentBlock::Thinking {
            thinking,
            signature,
        } => serde_json::json!({
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
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            let mapped_content: Vec<serde_json::Value> =
                content.iter().map(map_content_item).collect();
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

    let content_arr = body["content"].as_array().ok_or_else(|| {
        ProviderError::InvalidRequest("missing 'content' array in response".into())
    })?;

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
            let thinking = block["thinking"].as_str().unwrap_or_default().to_string();
            let signature = block["signature"].as_str().unwrap_or_default().to_string();
            Ok(ContentBlock::Thinking {
                thinking,
                signature,
            })
        }
        "redacted_thinking" => {
            let data = block["data"].as_str().unwrap_or_default().to_string();
            Ok(ContentBlock::RedactedThinking { data })
        }
        "tool_use" => {
            let id = block["id"]
                .as_str()
                .ok_or_else(|| ProviderError::InvalidRequest("tool_use block missing 'id'".into()))?
                .to_string();
            let name = block["name"]
                .as_str()
                .ok_or_else(|| {
                    ProviderError::InvalidRequest("tool_use block missing 'name'".into())
                })?
                .to_string();
            let input = block["input"].clone();
            Ok(ContentBlock::ToolUse { id, name, input })
        }
        "compaction" => {
            let content = block["content"].as_str().unwrap_or_default().to_string();
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
        cache_read_tokens: usage["cache_read_input_tokens"]
            .as_u64()
            .map(|n| n as usize),
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
        CacheControl, ContentBlock, ContentItem, ContextEdit, ContextManagement, DocumentSource,
        ImageSource, Message, Role, SystemBlock, SystemPrompt, ThinkingConfig, ToolDefinition,
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
        req.tool_choice = Some(ToolChoice::Specific {
            name: "search".into(),
        });
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

    // ─── Priority 1: Content block mapping tests ─────────────────────────────

    #[test]
    fn image_base64_content_block_maps_correctly() {
        let block = ContentBlock::Image {
            source: ImageSource::Base64 {
                media_type: "image/png".into(),
                data: "iVBORw0KGgo=".into(),
            },
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "image");
        assert_eq!(val["source"]["type"], "base64");
        assert_eq!(val["source"]["media_type"], "image/png");
        assert_eq!(val["source"]["data"], "iVBORw0KGgo=");
    }

    #[test]
    fn image_url_content_block_maps_correctly() {
        let block = ContentBlock::Image {
            source: ImageSource::Url {
                url: "https://example.com/image.png".into(),
            },
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "image");
        assert_eq!(val["source"]["type"], "url");
        assert_eq!(val["source"]["url"], "https://example.com/image.png");
    }

    #[test]
    fn document_base64_pdf_maps_correctly() {
        let block = ContentBlock::Document {
            source: DocumentSource::Base64Pdf {
                data: "JVBERi0xLjQ=".into(),
            },
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "document");
        assert_eq!(val["source"]["type"], "base64");
        assert_eq!(val["source"]["media_type"], "application/pdf");
        assert_eq!(val["source"]["data"], "JVBERi0xLjQ=");
    }

    #[test]
    fn document_plain_text_maps_correctly() {
        let block = ContentBlock::Document {
            source: DocumentSource::PlainText {
                data: "Hello, plain text document.".into(),
            },
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "document");
        assert_eq!(val["source"]["type"], "text");
        assert_eq!(val["source"]["data"], "Hello, plain text document.");
    }

    #[test]
    fn document_url_maps_correctly() {
        let block = ContentBlock::Document {
            source: DocumentSource::Url {
                url: "https://example.com/doc.pdf".into(),
            },
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "document");
        assert_eq!(val["source"]["type"], "url");
        assert_eq!(val["source"]["url"], "https://example.com/doc.pdf");
    }

    #[test]
    fn compaction_content_block_maps_correctly() {
        let block = ContentBlock::Compaction {
            content: "Compacted summary of the conversation.".into(),
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "compaction");
        assert_eq!(val["content"], "Compacted summary of the conversation.");
    }

    #[test]
    fn redacted_thinking_block_maps_correctly() {
        let block = ContentBlock::RedactedThinking {
            data: "opaque-redacted-data".into(),
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "redacted_thinking");
        assert_eq!(val["data"], "opaque-redacted-data");
    }

    #[test]
    fn image_in_tool_result_content_maps_correctly() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "toolu_img".into(),
            content: vec![
                ContentItem::Text("Here is the image:".into()),
                ContentItem::Image {
                    source: ImageSource::Base64 {
                        media_type: "image/jpeg".into(),
                        data: "/9j/4AAQ=".into(),
                    },
                },
            ],
            is_error: false,
        };
        let val = map_content_block(&block);
        assert_eq!(val["type"], "tool_result");
        assert_eq!(val["tool_use_id"], "toolu_img");

        let content = val["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Here is the image:");
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["type"], "base64");
        assert_eq!(content[1]["source"]["media_type"], "image/jpeg");
        assert_eq!(content[1]["source"]["data"], "/9j/4AAQ=");
    }

    #[test]
    fn context_management_maps_correctly() {
        let mut req = minimal_request();
        req.context_management = Some(ContextManagement {
            edits: vec![ContextEdit::Compact {
                strategy: "compact_20260112".into(),
            }],
        });
        let body = to_api_request(&req, "m");
        let ctx = &body["context_management"];
        let edits = ctx["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["type"], "auto");
        assert_eq!(edits[0]["trigger"], "compact_20260112");
    }

    #[test]
    fn context_management_with_multiple_edits() {
        let mut req = minimal_request();
        req.context_management = Some(ContextManagement {
            edits: vec![
                ContextEdit::Compact {
                    strategy: "strategy_a".into(),
                },
                ContextEdit::Compact {
                    strategy: "strategy_b".into(),
                },
            ],
        });
        let body = to_api_request(&req, "m");
        let edits = body["context_management"]["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0]["trigger"], "strategy_a");
        assert_eq!(edits[1]["trigger"], "strategy_b");
    }

    #[test]
    fn extra_fields_merged_into_body() {
        let mut req = minimal_request();
        req.extra = Some(serde_json::json!({
            "metadata": { "user_id": "user_123" },
            "custom_key": "custom_value",
        }));
        let body = to_api_request(&req, "m");
        assert_eq!(body["metadata"]["user_id"], "user_123");
        assert_eq!(body["custom_key"], "custom_value");
        // Original fields should still be present
        assert_eq!(body["model"], "m");
    }

    #[test]
    fn extra_fields_can_override_defaults() {
        let mut req = minimal_request();
        req.extra = Some(serde_json::json!({
            "max_tokens": 8192,
        }));
        let body = to_api_request(&req, "m");
        // Extra should override the default max_tokens
        assert_eq!(body["max_tokens"], 8192);
    }

    #[test]
    fn thinking_config_disabled_maps_correctly() {
        let mut req = minimal_request();
        req.thinking = Some(ThinkingConfig::Disabled);
        let body = to_api_request(&req, "m");
        assert_eq!(body["thinking"]["type"], "disabled");
    }

    #[test]
    fn thinking_config_adaptive_omits_thinking_field() {
        let mut req = minimal_request();
        req.thinking = Some(ThinkingConfig::Adaptive);
        let body = to_api_request(&req, "m");
        // Adaptive returns None from map_thinking_config, so "thinking" should not be in the body
        assert!(body.get("thinking").is_none() || body["thinking"].is_null());
    }

    #[test]
    fn thinking_config_enabled_maps_correctly() {
        let mut req = minimal_request();
        req.thinking = Some(ThinkingConfig::Enabled {
            budget_tokens: 10000,
        });
        let body = to_api_request(&req, "m");
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 10000);
    }

    // ─── Priority 2: Response parsing edge cases ─────────────────────────────

    #[test]
    fn parse_response_redacted_thinking() {
        let body = serde_json::json!({
            "id": "msg_redacted",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [
                { "type": "redacted_thinking", "data": "opaque-blob-abc123" },
                { "type": "text", "text": "Here is my answer." }
            ],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 50, "output_tokens": 30 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.message.content.len(), 2);
        assert!(
            matches!(&resp.message.content[0], ContentBlock::RedactedThinking { data } if data == "opaque-blob-abc123")
        );
        assert!(
            matches!(&resp.message.content[1], ContentBlock::Text(t) if t == "Here is my answer.")
        );
    }

    #[test]
    fn parse_response_compaction_block() {
        let body = serde_json::json!({
            "id": "msg_compact",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{
                "type": "compaction",
                "content": "Summary of previous context."
            }],
            "stop_reason": "compaction",
            "usage": { "input_tokens": 100, "output_tokens": 20 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::Compaction);
        assert!(
            matches!(&resp.message.content[0], ContentBlock::Compaction { content } if content == "Summary of previous context.")
        );
    }

    #[test]
    fn parse_response_unknown_content_block_type_returns_error() {
        let body = serde_json::json!({
            "id": "msg_unknown",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "some_new_type", "data": "whatever" }],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(
            matches!(&err, ProviderError::InvalidRequest(msg) if msg.contains("unknown content block type: some_new_type")),
            "expected InvalidRequest with unknown type, got: {err:?}"
        );
    }

    #[test]
    fn parse_stop_reason_stop_sequence() {
        let body = serde_json::json!({
            "id": "msg_stop_seq",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Partial output" }],
            "stop_reason": "stop_sequence",
            "usage": { "input_tokens": 10, "output_tokens": 5 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::StopSequence);
    }

    #[test]
    fn parse_stop_reason_max_tokens() {
        let body = serde_json::json!({
            "id": "msg_max_tok",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Truncated" }],
            "stop_reason": "max_tokens",
            "usage": { "input_tokens": 10, "output_tokens": 4096 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::MaxTokens);
    }

    #[test]
    fn parse_unknown_stop_reason_defaults_to_end_turn() {
        let body = serde_json::json!({
            "id": "msg_unknown_stop",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Done" }],
            "stop_reason": "some_future_reason",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn parse_null_stop_reason_defaults_to_end_turn() {
        let body = serde_json::json!({
            "id": "msg_null_stop",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Done" }],
            "stop_reason": null,
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn parse_usage_with_iterations() {
        let body = serde_json::json!({
            "id": "msg_iter",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Result after compaction" }],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 200,
                "output_tokens": 50,
                "iterations": [
                    {
                        "input_tokens": 150,
                        "output_tokens": 30,
                        "cache_read_input_tokens": 100,
                        "cache_creation_input_tokens": 50,
                    },
                    {
                        "input_tokens": 50,
                        "output_tokens": 20,
                    }
                ]
            }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.usage.input_tokens, 200);
        assert_eq!(resp.usage.output_tokens, 50);

        let iterations = resp.usage.iterations.as_ref().unwrap();
        assert_eq!(iterations.len(), 2);

        assert_eq!(iterations[0].input_tokens, 150);
        assert_eq!(iterations[0].output_tokens, 30);
        assert_eq!(iterations[0].cache_read_tokens, Some(100));
        assert_eq!(iterations[0].cache_creation_tokens, Some(50));

        assert_eq!(iterations[1].input_tokens, 50);
        assert_eq!(iterations[1].output_tokens, 20);
        assert_eq!(iterations[1].cache_read_tokens, None);
        assert_eq!(iterations[1].cache_creation_tokens, None);
    }

    #[test]
    fn parse_usage_without_iterations() {
        let body = serde_json::json!({
            "id": "msg_no_iter",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Hi" }],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 3,
            }
        });
        let resp = from_api_response(&body).unwrap();
        assert!(resp.usage.iterations.is_none());
    }

    #[test]
    fn parse_response_thinking_block() {
        let body = serde_json::json!({
            "id": "msg_think",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {
                    "type": "thinking",
                    "thinking": "Let me consider this carefully...",
                    "signature": "sig_abc123"
                },
                { "type": "text", "text": "Here is my answer." }
            ],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 30, "output_tokens": 50 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.message.content.len(), 2);
        assert!(
            matches!(&resp.message.content[0], ContentBlock::Thinking { thinking, signature } if thinking == "Let me consider this carefully..." && signature == "sig_abc123")
        );
    }

    #[test]
    fn parse_response_missing_id_returns_error() {
        let body = serde_json::json!({
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text", "text": "Hi" }],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg.contains("id")));
    }

    #[test]
    fn parse_response_missing_model_returns_error() {
        let body = serde_json::json!({
            "id": "msg_001",
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "text", "text": "Hi" }],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg.contains("model")));
    }

    #[test]
    fn parse_response_missing_content_array_returns_error() {
        let body = serde_json::json!({
            "id": "msg_001",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg.contains("content")));
    }

    #[test]
    fn parse_content_block_missing_type_returns_error() {
        let body = serde_json::json!({
            "id": "msg_001",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "text": "no type field" }],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg.contains("type")));
    }

    #[test]
    fn parse_tool_use_block_missing_id_returns_error() {
        let body = serde_json::json!({
            "id": "msg_001",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{
                "type": "tool_use",
                "name": "search",
                "input": {}
            }],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg.contains("id")));
    }

    #[test]
    fn parse_tool_use_block_missing_name_returns_error() {
        let body = serde_json::json!({
            "id": "msg_001",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{
                "type": "tool_use",
                "id": "toolu_01",
                "input": {}
            }],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg.contains("name")));
    }

    #[test]
    fn parse_text_block_missing_text_returns_error() {
        let body = serde_json::json!({
            "id": "msg_001",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [{ "type": "text" }],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(msg) if msg.contains("text")));
    }

    // ─── Request mapping: optional parameters ────────────────────────────────

    #[test]
    fn temperature_maps_correctly() {
        let mut req = minimal_request();
        req.temperature = Some(0.7);
        let body = to_api_request(&req, "m");
        let temp = body["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 0.001, "expected ~0.7, got {temp}");
    }

    #[test]
    fn top_p_maps_correctly() {
        let mut req = minimal_request();
        req.top_p = Some(0.9);
        let body = to_api_request(&req, "m");
        let top_p = body["top_p"].as_f64().unwrap();
        assert!((top_p - 0.9).abs() < 0.001, "expected ~0.9, got {top_p}");
    }

    #[test]
    fn stop_sequences_map_correctly() {
        let mut req = minimal_request();
        req.stop_sequences = vec!["STOP".into(), "END".into()];
        let body = to_api_request(&req, "m");
        let seqs = body["stop_sequences"].as_array().unwrap();
        assert_eq!(seqs.len(), 2);
        assert_eq!(seqs[0], "STOP");
        assert_eq!(seqs[1], "END");
    }

    #[test]
    fn max_tokens_defaults_to_4096() {
        let req = minimal_request();
        let body = to_api_request(&req, "m");
        assert_eq!(body["max_tokens"], 4096);
    }

    #[test]
    fn max_tokens_override() {
        let mut req = minimal_request();
        req.max_tokens = Some(1024);
        let body = to_api_request(&req, "m");
        assert_eq!(body["max_tokens"], 1024);
    }

    #[test]
    fn tool_definition_with_cache_control() {
        let mut req = minimal_request();
        req.tools = vec![ToolDefinition {
            name: "cached_tool".into(),
            title: None,
            description: "A cached tool".into(),
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            annotations: None,
            cache_control: Some(CacheControl {
                ttl: Some(neuron_types::CacheTtl::FiveMinutes),
            }),
        }];
        let body = to_api_request(&req, "m");
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn system_block_without_cache_control() {
        let mut req = minimal_request();
        req.system = Some(SystemPrompt::Blocks(vec![SystemBlock {
            text: "No cache.".into(),
            cache_control: None,
        }]));
        let body = to_api_request(&req, "m");
        let system = body["system"].as_array().unwrap();
        assert_eq!(system[0]["text"], "No cache.");
        assert!(system[0].get("cache_control").is_none());
    }

    #[test]
    fn multiple_messages_preserve_order() {
        let mut req = minimal_request();
        req.messages = vec![
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("First".into())],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text("Response".into())],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("Second".into())],
            },
        ];
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[2]["role"], "user");
    }
}
