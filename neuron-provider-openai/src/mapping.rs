//! Request/response mapping between neuron-types and the OpenAI Chat Completions API format.
//!
//! Reference: <https://platform.openai.com/docs/api-reference/chat>

use neuron_types::{
    CompletionRequest, CompletionResponse, ContentBlock, ContentItem, Message, ProviderError,
    ReasoningEffort, ResponseFormat, Role, StopReason, SystemPrompt, TokenUsage, ToolChoice,
    ToolDefinition,
};

// ─── Request mapping ─────────────────────────────────────────────────────────

/// Convert a [`CompletionRequest`] into the OpenAI Chat Completions API JSON body.
///
/// The returned value does **not** include `"stream"` — callers add that key.
#[must_use]
pub fn to_api_request(req: &CompletionRequest, default_model: &str) -> serde_json::Value {
    let model = if req.model.is_empty() {
        default_model.to_string()
    } else {
        req.model.clone()
    };

    let mut messages = Vec::new();

    // System prompt goes first as a "developer" role message
    if let Some(system) = &req.system {
        messages.extend(map_system_prompt(system));
    }

    // Map conversation messages
    messages.extend(map_messages(&req.messages));

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
    });

    // Max tokens — use max_completion_tokens for newer API
    if let Some(max_tokens) = req.max_tokens {
        body["max_completion_tokens"] = serde_json::Value::from(max_tokens);
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
        body["stop"] = serde_json::Value::Array(
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

    // Response format (structured output)
    if let Some(format) = &req.response_format {
        body["response_format"] = map_response_format(format);
    }

    // Reasoning effort for o-series models
    if let Some(effort) = &req.reasoning_effort {
        body["reasoning_effort"] = map_reasoning_effort(effort);
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

/// Map a [`SystemPrompt`] to OpenAI message(s) with `role: "developer"`.
fn map_system_prompt(system: &SystemPrompt) -> Vec<serde_json::Value> {
    match system {
        SystemPrompt::Text(text) => {
            vec![serde_json::json!({
                "role": "developer",
                "content": text,
            })]
        }
        SystemPrompt::Blocks(blocks) => {
            let content: Vec<serde_json::Value> = blocks
                .iter()
                .map(|block| {
                    serde_json::json!({
                        "type": "text",
                        "text": block.text,
                    })
                })
                .collect();
            vec![serde_json::json!({
                "role": "developer",
                "content": content,
            })]
        }
    }
}

/// Map a list of [`Message`]s to OpenAI message array format.
///
/// OpenAI uses a different format for tool calls and tool results:
/// - Assistant messages with `ToolUse` blocks become `tool_calls` on the message
/// - `ToolResult` blocks become separate messages with `role: "tool"`
fn map_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    let mut result = Vec::new();

    for msg in messages {
        // System messages should be handled via SystemPrompt; skip inline ones.
        if msg.role == Role::System {
            continue;
        }

        let role_str = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => unreachable!("filtered above"),
        };

        if msg.role == Role::Assistant {
            // Assistant messages: separate text content from tool calls.
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            for block in &msg.content {
                match block {
                    ContentBlock::Text(text) => {
                        text_parts.push(text.clone());
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        let arguments = match input {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        tool_calls.push(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments,
                            },
                        }));
                    }
                    _ => {
                        // Thinking, RedactedThinking, etc. are not sent to OpenAI
                    }
                }
            }

            let mut msg_obj = serde_json::json!({ "role": role_str });

            if !text_parts.is_empty() {
                msg_obj["content"] = serde_json::Value::String(text_parts.join(""));
            } else {
                msg_obj["content"] = serde_json::Value::Null;
            }

            if !tool_calls.is_empty() {
                msg_obj["tool_calls"] = serde_json::Value::Array(tool_calls);
            }

            result.push(msg_obj);
        } else {
            // User messages: may contain text, images, tool results.
            // Tool results become separate "tool" role messages.
            let mut content_parts = Vec::new();
            let mut tool_results = Vec::new();

            for block in &msg.content {
                match block {
                    ContentBlock::Text(text) => {
                        content_parts.push(serde_json::json!({
                            "type": "text",
                            "text": text,
                        }));
                    }
                    ContentBlock::Image { source } => {
                        content_parts.push(map_image_block(source));
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } => {
                        let text_content: String = content
                            .iter()
                            .filter_map(|item| match item {
                                ContentItem::Text(t) => Some(t.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        tool_results.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": text_content,
                        }));
                    }
                    _ => {
                        // Skip unsupported blocks (ToolUse on user, etc.)
                    }
                }
            }

            if !content_parts.is_empty() {
                if content_parts.len() == 1 && content_parts[0]["type"].as_str() == Some("text") {
                    // Single text block: use string content for simplicity
                    result.push(serde_json::json!({
                        "role": role_str,
                        "content": content_parts[0]["text"],
                    }));
                } else {
                    result.push(serde_json::json!({
                        "role": role_str,
                        "content": content_parts,
                    }));
                }
            }

            // Tool results go as separate messages after the user message
            result.extend(tool_results);
        }
    }

    result
}

/// Map an [`ImageSource`] to an OpenAI image content block.
fn map_image_block(source: &neuron_types::ImageSource) -> serde_json::Value {
    match source {
        neuron_types::ImageSource::Base64 { media_type, data } => {
            serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:{media_type};base64,{data}"),
                },
            })
        }
        neuron_types::ImageSource::Url { url } => {
            serde_json::json!({
                "type": "image_url",
                "image_url": { "url": url },
            })
        }
    }
}

/// Map a [`ToolDefinition`] to OpenAI's function tool format.
fn map_tool_definition(tool: &ToolDefinition) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        },
    })
}

/// Map a [`ToolChoice`] to OpenAI's tool_choice format.
fn map_tool_choice(choice: &ToolChoice) -> serde_json::Value {
    match choice {
        ToolChoice::Auto => serde_json::Value::String("auto".into()),
        ToolChoice::None => serde_json::Value::String("none".into()),
        ToolChoice::Required => serde_json::Value::String("required".into()),
        ToolChoice::Specific { name } => serde_json::json!({
            "type": "function",
            "function": { "name": name },
        }),
    }
}

/// Map a [`ResponseFormat`] to OpenAI's response_format object.
fn map_response_format(format: &ResponseFormat) -> serde_json::Value {
    match format {
        ResponseFormat::Text => serde_json::json!({ "type": "text" }),
        ResponseFormat::JsonObject => serde_json::json!({ "type": "json_object" }),
        ResponseFormat::JsonSchema {
            name,
            schema,
            strict,
        } => serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": name,
                "schema": schema,
                "strict": strict,
            },
        }),
    }
}

/// Map a [`ReasoningEffort`] to OpenAI's reasoning_effort string.
fn map_reasoning_effort(effort: &ReasoningEffort) -> serde_json::Value {
    let s = match effort {
        ReasoningEffort::None => "none",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
    };
    serde_json::Value::String(s.into())
}

// ─── Response mapping ─────────────────────────────────────────────────────────

/// Parse an OpenAI Chat Completions API response JSON into a [`CompletionResponse`].
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

    let choice = body["choices"]
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| {
            ProviderError::InvalidRequest("missing 'choices' array in response".into())
        })?;

    let message = &choice["message"];
    let mut content_blocks = Vec::new();

    // Parse text content
    if let Some(text) = message["content"].as_str()
        && !text.is_empty()
    {
        content_blocks.push(ContentBlock::Text(text.to_string()));
    }

    // Parse tool calls
    if let Some(tool_calls) = message["tool_calls"].as_array() {
        for tc in tool_calls {
            let tc_id = tc["id"].as_str().unwrap_or_default().to_string();
            let name = tc["function"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let arguments_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
            let input: serde_json::Value = serde_json::from_str(arguments_str)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            content_blocks.push(ContentBlock::ToolUse {
                id: tc_id,
                name,
                input,
            });
        }
    }

    let usage = parse_usage(&body["usage"]);

    let finish_reason = choice["finish_reason"]
        .as_str()
        .map(parse_finish_reason)
        .unwrap_or(StopReason::EndTurn);

    Ok(CompletionResponse {
        id,
        model,
        message: Message {
            role: Role::Assistant,
            content: content_blocks,
        },
        usage,
        stop_reason: finish_reason,
    })
}

/// Parse [`TokenUsage`] from the OpenAI response `usage` field.
fn parse_usage(usage: &serde_json::Value) -> TokenUsage {
    TokenUsage {
        input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as usize,
        output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as usize,
        cache_read_tokens: usage["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .map(|n| n as usize),
        cache_creation_tokens: None,
        reasoning_tokens: usage["completion_tokens_details"]["reasoning_tokens"]
            .as_u64()
            .map(|n| n as usize),
        iterations: None,
    }
}

/// Map an OpenAI `finish_reason` string to a [`StopReason`].
fn parse_finish_reason(reason: &str) -> StopReason {
    match reason {
        "stop" => StopReason::EndTurn,
        "tool_calls" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        "content_filter" => StopReason::ContentFilter,
        _ => StopReason::EndTurn,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use neuron_types::{
        CacheControl, ContentBlock, ContentItem, Message, ReasoningEffort, ResponseFormat, Role,
        SystemBlock, SystemPrompt, ToolChoice, ToolDefinition,
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
        let body = to_api_request(&req, "gpt-4o");
        assert_eq!(body["model"], "gpt-4o");
    }

    #[test]
    fn explicit_model_takes_precedence() {
        let mut req = minimal_request();
        req.model = "gpt-4o-mini".into();
        let body = to_api_request(&req, "gpt-4o");
        assert_eq!(body["model"], "gpt-4o-mini");
    }

    #[test]
    fn messages_mapped_correctly() {
        let req = minimal_request();
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "Hello");
    }

    #[test]
    fn system_text_prompt_mapped_as_developer_role() {
        let mut req = minimal_request();
        req.system = Some(SystemPrompt::Text("You are a helpful assistant.".into()));
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "developer");
        assert_eq!(messages[0]["content"], "You are a helpful assistant.");
    }

    #[test]
    fn system_blocks_prompt_mapped_as_developer_role_with_array() {
        let mut req = minimal_request();
        req.system = Some(SystemPrompt::Blocks(vec![SystemBlock {
            text: "Be concise.".into(),
            cache_control: Some(CacheControl { ttl: None }),
        }]));
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "developer");
        let content = messages[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Be concise.");
    }

    #[test]
    fn tool_definition_mapped_as_function() {
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
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "search");
        assert_eq!(tools[0]["function"]["description"], "Search the web");
        assert!(tools[0]["function"]["parameters"].is_object());
    }

    #[test]
    fn tool_choice_auto_maps_to_string() {
        let mut req = minimal_request();
        req.tool_choice = Some(ToolChoice::Auto);
        let body = to_api_request(&req, "m");
        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn tool_choice_none_maps_to_string() {
        let mut req = minimal_request();
        req.tool_choice = Some(ToolChoice::None);
        let body = to_api_request(&req, "m");
        assert_eq!(body["tool_choice"], "none");
    }

    #[test]
    fn tool_choice_required_maps_to_string() {
        let mut req = minimal_request();
        req.tool_choice = Some(ToolChoice::Required);
        let body = to_api_request(&req, "m");
        assert_eq!(body["tool_choice"], "required");
    }

    #[test]
    fn tool_choice_specific_maps_correctly() {
        let mut req = minimal_request();
        req.tool_choice = Some(ToolChoice::Specific {
            name: "search".into(),
        });
        let body = to_api_request(&req, "m");
        assert_eq!(body["tool_choice"]["type"], "function");
        assert_eq!(body["tool_choice"]["function"]["name"], "search");
    }

    #[test]
    fn response_format_json_schema_mapped() {
        let mut req = minimal_request();
        req.response_format = Some(ResponseFormat::JsonSchema {
            name: "answer".into(),
            schema: serde_json::json!({ "type": "object" }),
            strict: true,
        });
        let body = to_api_request(&req, "m");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["name"], "answer");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    }

    #[test]
    fn reasoning_effort_mapped() {
        let mut req = minimal_request();
        req.reasoning_effort = Some(ReasoningEffort::High);
        let body = to_api_request(&req, "m");
        assert_eq!(body["reasoning_effort"], "high");
    }

    #[test]
    fn max_tokens_uses_max_completion_tokens() {
        let mut req = minimal_request();
        req.max_tokens = Some(1024);
        let body = to_api_request(&req, "m");
        assert_eq!(body["max_completion_tokens"], 1024);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn assistant_tool_use_maps_to_tool_calls() {
        let mut req = minimal_request();
        req.messages.push(Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "call_abc".into(),
                name: "search".into(),
                input: serde_json::json!({ "q": "rust" }),
            }],
        });
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        let assistant_msg = &messages[1];
        assert_eq!(assistant_msg["role"], "assistant");
        let tool_calls = assistant_msg["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["id"], "call_abc");
        assert_eq!(tool_calls[0]["type"], "function");
        assert_eq!(tool_calls[0]["function"]["name"], "search");
    }

    #[test]
    fn tool_result_maps_to_tool_role() {
        let mut req = minimal_request();
        req.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "call_abc".into(),
                content: vec![ContentItem::Text("found it".into())],
                is_error: false,
            }],
        });
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        // Tool results go as separate tool role messages
        let tool_msg = &messages[1];
        assert_eq!(tool_msg["role"], "tool");
        assert_eq!(tool_msg["tool_call_id"], "call_abc");
        assert_eq!(tool_msg["content"], "found it");
    }

    #[test]
    fn parse_response_text_only() {
        let body = serde_json::json!({
            "id": "chatcmpl-abc123",
            "object": "chat.completion",
            "model": "gpt-4o-2024-08-06",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.id, "chatcmpl-abc123");
        assert_eq!(resp.model, "gpt-4o-2024-08-06");
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert!(matches!(&resp.message.content[0], ContentBlock::Text(t) if t == "Hello!"));
    }

    #[test]
    fn parse_response_tool_calls() {
        let body = serde_json::json!({
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{\"query\":\"rust\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 15,
                "total_tokens": 35
            }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::ToolUse);
        assert!(matches!(
            &resp.message.content[0],
            ContentBlock::ToolUse { id, name, input }
                if id == "call_abc123" && name == "search" && input["query"] == "rust"
        ));
    }

    #[test]
    fn parse_response_with_reasoning_tokens() {
        let body = serde_json::json!({
            "id": "chatcmpl-reason",
            "object": "chat.completion",
            "model": "o1-preview",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "The answer is 42."
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 50,
                "completion_tokens": 100,
                "total_tokens": 150,
                "completion_tokens_details": {
                    "reasoning_tokens": 80
                }
            }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.usage.reasoning_tokens, Some(80));
    }

    #[test]
    fn parse_response_with_cached_tokens() {
        let body = serde_json::json!({
            "id": "chatcmpl-cached",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hi"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 5,
                "total_tokens": 105,
                "prompt_tokens_details": {
                    "cached_tokens": 80
                }
            }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.usage.cache_read_tokens, Some(80));
    }

    #[test]
    fn parse_finish_reason_length() {
        let body = serde_json::json!({
            "id": "chatcmpl-len",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "..." },
                "finish_reason": "length"
            }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 4096, "total_tokens": 4101 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::MaxTokens);
    }

    #[test]
    fn parse_finish_reason_content_filter() {
        let body = serde_json::json!({
            "id": "chatcmpl-filter",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "" },
                "finish_reason": "content_filter"
            }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 0, "total_tokens": 5 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::ContentFilter);
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
        // System message should be filtered out; only user message remains
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn extra_fields_forwarded() {
        let mut req = minimal_request();
        req.extra = Some(serde_json::json!({ "seed": 42, "logprobs": true }));
        let body = to_api_request(&req, "m");
        assert_eq!(body["seed"], 42);
        assert_eq!(body["logprobs"], true);
    }

    #[test]
    fn stop_sequences_mapped_to_stop() {
        let mut req = minimal_request();
        req.stop_sequences = vec!["END".into(), "STOP".into()];
        let body = to_api_request(&req, "m");
        let stop = body["stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0], "END");
        assert_eq!(stop[1], "STOP");
    }

    // ─── ResponseFormat variants ──────────────────────────────────────────

    #[test]
    fn response_format_text_mapped() {
        let mut req = minimal_request();
        req.response_format = Some(ResponseFormat::Text);
        let body = to_api_request(&req, "m");
        assert_eq!(body["response_format"]["type"], "text");
    }

    #[test]
    fn response_format_json_object_mapped() {
        let mut req = minimal_request();
        req.response_format = Some(ResponseFormat::JsonObject);
        let body = to_api_request(&req, "m");
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    // ─── ReasoningEffort variants ─────────────────────────────────────────

    #[test]
    fn reasoning_effort_none_mapped() {
        let mut req = minimal_request();
        req.reasoning_effort = Some(ReasoningEffort::None);
        let body = to_api_request(&req, "m");
        assert_eq!(body["reasoning_effort"], "none");
    }

    #[test]
    fn reasoning_effort_low_mapped() {
        let mut req = minimal_request();
        req.reasoning_effort = Some(ReasoningEffort::Low);
        let body = to_api_request(&req, "m");
        assert_eq!(body["reasoning_effort"], "low");
    }

    #[test]
    fn reasoning_effort_medium_mapped() {
        let mut req = minimal_request();
        req.reasoning_effort = Some(ReasoningEffort::Medium);
        let body = to_api_request(&req, "m");
        assert_eq!(body["reasoning_effort"], "medium");
    }

    // ─── Image content blocks ─────────────────────────────────────────────

    #[test]
    fn image_base64_mapped_to_image_url_with_data_uri() {
        let mut req = minimal_request();
        req.messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Image {
                source: neuron_types::ImageSource::Base64 {
                    media_type: "image/png".into(),
                    data: "iVBORw0KGgo=".into(),
                },
            }],
        }];
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        // Single image block produces array form content
        let content = messages[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "image_url");
        assert_eq!(
            content[0]["image_url"]["url"],
            "data:image/png;base64,iVBORw0KGgo="
        );
    }

    #[test]
    fn image_url_mapped_to_image_url() {
        let mut req = minimal_request();
        req.messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Image {
                source: neuron_types::ImageSource::Url {
                    url: "https://example.com/photo.jpg".into(),
                },
            }],
        }];
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        let content = messages[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "image_url");
        assert_eq!(
            content[0]["image_url"]["url"],
            "https://example.com/photo.jpg"
        );
    }

    // ─── Multi-content user messages ──────────────────────────────────────

    #[test]
    fn multi_content_user_message_generates_array_form() {
        let mut req = minimal_request();
        req.messages = vec![Message {
            role: Role::User,
            content: vec![
                ContentBlock::Text("Describe this image:".into()),
                ContentBlock::Image {
                    source: neuron_types::ImageSource::Url {
                        url: "https://example.com/cat.png".into(),
                    },
                },
            ],
        }];
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        // Multi-content produces array form
        let content = messages[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Describe this image:");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(
            content[1]["image_url"]["url"],
            "https://example.com/cat.png"
        );
    }

    // ─── Assistant messages with mixed content ────────────────────────────

    #[test]
    fn assistant_text_and_tool_use_combined() {
        let mut req = minimal_request();
        req.messages.push(Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text("Let me search for that.".into()),
                ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "search".into(),
                    input: serde_json::json!({ "q": "rust" }),
                },
            ],
        });
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        let assistant_msg = &messages[1];
        assert_eq!(assistant_msg["content"], "Let me search for that.");
        let tool_calls = assistant_msg["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["function"]["name"], "search");
    }

    #[test]
    fn assistant_tool_use_with_string_input() {
        let mut req = minimal_request();
        req.messages.push(Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "call_str".into(),
                name: "eval".into(),
                input: serde_json::Value::String("{\"code\":\"1+1\"}".into()),
            }],
        });
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        let tc = &messages[1]["tool_calls"].as_array().unwrap()[0];
        // String input should pass through directly, not re-stringify
        assert_eq!(tc["function"]["arguments"], "{\"code\":\"1+1\"}");
    }

    #[test]
    fn assistant_without_text_has_null_content() {
        let mut req = minimal_request();
        req.messages.push(Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "search".into(),
                input: serde_json::json!({}),
            }],
        });
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        assert!(messages[1]["content"].is_null());
    }

    #[test]
    fn assistant_thinking_blocks_are_skipped() {
        let mut req = minimal_request();
        req.messages.push(Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    thinking: "hmm...".into(),
                    signature: "sig".into(),
                },
                ContentBlock::Text("Answer.".into()),
            ],
        });
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        let assistant_msg = &messages[1];
        // Only text should be present, thinking should be skipped
        assert_eq!(assistant_msg["content"], "Answer.");
        assert!(assistant_msg.get("tool_calls").is_none() || assistant_msg["tool_calls"].is_null());
    }

    // ─── Tool result with multiple text items ─────────────────────────────

    #[test]
    fn tool_result_joins_multiple_text_items() {
        let mut req = minimal_request();
        req.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".into(),
                content: vec![
                    ContentItem::Text("line one".into()),
                    ContentItem::Text("line two".into()),
                ],
                is_error: false,
            }],
        });
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        let tool_msg = &messages[1];
        assert_eq!(tool_msg["role"], "tool");
        assert_eq!(tool_msg["content"], "line one\nline two");
    }

    // ─── Response parsing edge cases ──────────────────────────────────────

    #[test]
    fn parse_response_missing_id_errors() {
        let body = serde_json::json!({
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hi" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    #[test]
    fn parse_response_missing_model_errors() {
        let body = serde_json::json!({
            "id": "chatcmpl-abc",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hi" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    #[test]
    fn parse_response_missing_choices_errors() {
        let body = serde_json::json!({
            "id": "chatcmpl-abc",
            "model": "gpt-4o"
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    #[test]
    fn parse_response_empty_choices_errors() {
        let body = serde_json::json!({
            "id": "chatcmpl-abc",
            "model": "gpt-4o",
            "choices": [],
            "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 }
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    #[test]
    fn parse_finish_reason_unknown_defaults_to_end_turn() {
        let body = serde_json::json!({
            "id": "chatcmpl-unk",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hi" },
                "finish_reason": "some_unknown_reason"
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn parse_finish_reason_null_defaults_to_end_turn() {
        let body = serde_json::json!({
            "id": "chatcmpl-null",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hi" },
                "finish_reason": null
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn parse_usage_missing_fields_default_to_zero() {
        let body = serde_json::json!({
            "id": "chatcmpl-nousage",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hi" },
                "finish_reason": "stop"
            }],
            "usage": {}
        });
        let resp = from_api_response(&body).unwrap();
        assert_eq!(resp.usage.input_tokens, 0);
        assert_eq!(resp.usage.output_tokens, 0);
        assert!(resp.usage.cache_read_tokens.is_none());
        assert!(resp.usage.reasoning_tokens.is_none());
    }

    #[test]
    fn parse_response_tool_call_with_invalid_json_arguments() {
        let body = serde_json::json!({
            "id": "chatcmpl-bad-args",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_bad",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "not-valid-json"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        });
        // Should not error — falls back to empty object
        let resp = from_api_response(&body).unwrap();
        assert!(matches!(
            &resp.message.content[0],
            ContentBlock::ToolUse { input, .. } if input.is_object()
        ));
    }

    #[test]
    fn temperature_and_top_p_mapped() {
        let mut req = minimal_request();
        req.temperature = Some(0.5);
        req.top_p = Some(1.0);
        let body = to_api_request(&req, "m");
        // Use values that are exactly representable in f32 to avoid precision issues
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["top_p"], 1.0);
    }

    #[test]
    fn user_message_with_unsupported_block_is_skipped() {
        let mut req = minimal_request();
        req.messages = vec![Message {
            role: Role::User,
            content: vec![
                ContentBlock::Text("Hello".into()),
                // ToolUse on user role — should be skipped
                ContentBlock::ToolUse {
                    id: "id".into(),
                    name: "name".into(),
                    input: serde_json::json!({}),
                },
            ],
        }];
        let body = to_api_request(&req, "m");
        let messages = body["messages"].as_array().unwrap();
        // Should only produce the text content (string form since single text)
        assert_eq!(messages[0]["content"], "Hello");
    }

    #[test]
    fn extra_non_object_is_ignored() {
        let mut req = minimal_request();
        req.extra = Some(serde_json::json!("a string, not an object"));
        let body = to_api_request(&req, "m");
        // Should not crash, and body should still have model and messages
        assert_eq!(body["model"], "m");
        assert!(body["messages"].as_array().is_some());
    }
}
