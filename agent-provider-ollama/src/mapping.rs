//! Request/response mapping between agent-types and the Ollama Chat API format.
//!
//! Reference: <https://github.com/ollama/ollama/blob/main/docs/api.md#generate-a-chat-completion>

use agent_types::{
    CompletionRequest, CompletionResponse, ContentBlock, ContentItem, Message, ProviderError, Role,
    StopReason, SystemPrompt, TokenUsage, ToolDefinition,
};

// ─── Request mapping ─────────────────────────────────────────────────────────

/// Convert a [`CompletionRequest`] into the Ollama Chat API JSON body.
///
/// The returned value does **not** include `"stream"` — callers add that key.
pub fn to_api_request(
    req: &CompletionRequest,
    default_model: &str,
    keep_alive: Option<&str>,
) -> serde_json::Value {
    let model = if req.model.is_empty() {
        default_model.to_string()
    } else {
        req.model.clone()
    };

    let mut messages = map_messages(&req.messages);

    // Ollama accepts system messages inline, but also supports a dedicated system prompt.
    // We prepend a system message if configured.
    if let Some(system) = &req.system {
        let system_text = match system {
            SystemPrompt::Text(text) => text.clone(),
            SystemPrompt::Blocks(blocks) => blocks
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        };
        let system_msg = serde_json::json!({
            "role": "system",
            "content": system_text,
        });
        messages.insert(0, system_msg);
    }

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
    });

    // Build options object for model parameters
    let mut options = serde_json::Map::new();

    if let Some(max_tokens) = req.max_tokens {
        options.insert("num_predict".into(), serde_json::Value::from(max_tokens));
    }

    if let Some(temp) = req.temperature {
        options.insert(
            "temperature".into(),
            serde_json::Value::from(temp as f64),
        );
    }

    if let Some(top_p) = req.top_p {
        options.insert("top_p".into(), serde_json::Value::from(top_p as f64));
    }

    if !req.stop_sequences.is_empty() {
        options.insert(
            "stop".into(),
            serde_json::Value::Array(
                req.stop_sequences
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            ),
        );
    }

    // Merge extra fields into options (provider-specific Ollama options)
    if let Some(serde_json::Value::Object(extra_map)) = &req.extra {
        for (k, v) in extra_map {
            options.insert(k.clone(), v.clone());
        }
    }

    if !options.is_empty() {
        body["options"] = serde_json::Value::Object(options);
    }

    // Tools — Ollama uses the same format as OpenAI
    if !req.tools.is_empty() {
        body["tools"] = serde_json::Value::Array(
            req.tools.iter().map(map_tool_definition).collect(),
        );
    }

    // Tool choice — Ollama doesn't have direct tool_choice support in the same way,
    // but we include it for forward compatibility
    // (Ollama ignores unknown fields gracefully)

    // keep_alive
    if let Some(ka) = keep_alive {
        body["keep_alive"] = serde_json::Value::String(ka.to_string());
    }

    body
}

/// Map a list of [`Message`]s to Ollama's message array format.
fn map_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|msg| {
            let role_str = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };

            let mut message = serde_json::json!({ "role": role_str });

            // Extract text content
            let text_content = extract_text_content(&msg.content);
            if !text_content.is_empty() {
                message["content"] = serde_json::Value::String(text_content);
            }

            // Extract tool calls (for assistant messages)
            let tool_calls = extract_tool_calls(&msg.content);
            if !tool_calls.is_empty() {
                message["tool_calls"] = serde_json::Value::Array(tool_calls);
            }

            message
        })
        .collect()
}

/// Extract text content from content blocks into a single string.
fn extract_text_content(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.as_str()),
            ContentBlock::ToolResult {
                content, is_error, ..
            } => {
                // Include tool results as text
                let text = content
                    .iter()
                    .filter_map(|item| match item {
                        ContentItem::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    Some(if *is_error { "Error: " } else { "" })
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Extract tool calls from content blocks for Ollama format.
fn extract_tool_calls(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { name, input, .. } => Some(serde_json::json!({
                "function": {
                    "name": name,
                    "arguments": input,
                }
            })),
            _ => None,
        })
        .collect()
}

/// Map a [`ToolDefinition`] to Ollama's tool definition format (OpenAI-compatible).
fn map_tool_definition(tool: &ToolDefinition) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

// ─── Response mapping ─────────────────────────────────────────────────────────

/// Parse an Ollama Chat API response JSON into a [`CompletionResponse`].
///
/// # Errors
///
/// Returns [`ProviderError::InvalidRequest`] if required fields are missing or malformed.
pub fn from_api_response(body: &serde_json::Value) -> Result<CompletionResponse, ProviderError> {
    let model = body["model"]
        .as_str()
        .ok_or_else(|| ProviderError::InvalidRequest("missing 'model' in response".into()))?
        .to_string();

    let message_obj = &body["message"];

    // Parse text content
    let content_text = message_obj["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let mut content_blocks = Vec::new();

    if !content_text.is_empty() {
        content_blocks.push(ContentBlock::Text(content_text));
    }

    // Parse tool calls (Ollama uses same format as OpenAI)
    if let Some(tool_calls) = message_obj["tool_calls"].as_array() {
        for tc in tool_calls {
            let function = &tc["function"];
            let name = function["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let arguments = &function["arguments"];

            // Ollama does not provide tool call IDs; synthesize one
            let id = format!("ollama_{}", uuid::Uuid::new_v4());

            content_blocks.push(ContentBlock::ToolUse {
                id,
                name,
                input: arguments.clone(),
            });
        }
    }

    // Parse usage from Ollama's eval fields
    let usage = parse_usage(body);

    // Determine stop reason
    let stop_reason = parse_stop_reason(body);

    // Ollama doesn't return a message ID; synthesize one
    let id = format!("ollama_{}", uuid::Uuid::new_v4());

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

/// Parse [`TokenUsage`] from the Ollama response.
///
/// Ollama provides `eval_count` (output tokens) and `prompt_eval_count` (input tokens).
fn parse_usage(body: &serde_json::Value) -> TokenUsage {
    TokenUsage {
        input_tokens: body["prompt_eval_count"].as_u64().unwrap_or(0) as usize,
        output_tokens: body["eval_count"].as_u64().unwrap_or(0) as usize,
        cache_read_tokens: None,
        cache_creation_tokens: None,
        reasoning_tokens: None,
    }
}

/// Parse the stop reason from Ollama's `done_reason` field.
fn parse_stop_reason(body: &serde_json::Value) -> StopReason {
    match body["done_reason"].as_str() {
        Some("stop") => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        Some("tool_calls") => StopReason::ToolUse,
        _ => {
            // If there are tool calls in the message, treat as ToolUse
            if body["message"]["tool_calls"].is_array()
                && !body["message"]["tool_calls"]
                    .as_array()
                    .map_or(true, |a| a.is_empty())
            {
                StopReason::ToolUse
            } else {
                StopReason::EndTurn
            }
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use agent_types::{
        ContentBlock, Message, Role, SystemPrompt, ToolDefinition,
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
        }
    }

    #[test]
    fn minimal_request_uses_default_model() {
        let req = minimal_request();
        let body = to_api_request(&req, "llama3.2", None);
        assert_eq!(body["model"], "llama3.2");
    }

    #[test]
    fn explicit_model_takes_precedence() {
        let mut req = minimal_request();
        req.model = "mistral".into();
        let body = to_api_request(&req, "llama3.2", None);
        assert_eq!(body["model"], "mistral");
    }

    #[test]
    fn messages_mapped_correctly() {
        let req = minimal_request();
        let body = to_api_request(&req, "m", None);
        let messages = body["messages"].as_array().expect("messages should be array");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "Hello");
    }

    #[test]
    fn system_text_prompt_prepended_as_system_message() {
        let mut req = minimal_request();
        req.system = Some(SystemPrompt::Text("You are a helpful assistant.".into()));
        let body = to_api_request(&req, "m", None);
        let messages = body["messages"].as_array().expect("messages should be array");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are a helpful assistant.");
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn system_blocks_joined_as_single_system_message() {
        let mut req = minimal_request();
        req.system = Some(SystemPrompt::Blocks(vec![
            agent_types::SystemBlock {
                text: "Be concise.".into(),
                cache_control: None,
            },
            agent_types::SystemBlock {
                text: "Be helpful.".into(),
                cache_control: None,
            },
        ]));
        let body = to_api_request(&req, "m", None);
        let messages = body["messages"].as_array().expect("messages should be array");
        assert_eq!(messages[0]["content"], "Be concise.\n\nBe helpful.");
    }

    #[test]
    fn max_tokens_maps_to_num_predict() {
        let mut req = minimal_request();
        req.max_tokens = Some(256);
        let body = to_api_request(&req, "m", None);
        assert_eq!(body["options"]["num_predict"], 256);
    }

    #[test]
    fn temperature_maps_to_options() {
        let mut req = minimal_request();
        req.temperature = Some(0.7);
        let body = to_api_request(&req, "m", None);
        let temp = body["options"]["temperature"].as_f64().expect("should be f64");
        assert!((temp - 0.7).abs() < 0.001, "expected ~0.7, got {temp}");
    }

    #[test]
    fn top_p_maps_to_options() {
        let mut req = minimal_request();
        req.top_p = Some(0.9);
        let body = to_api_request(&req, "m", None);
        let top_p = body["options"]["top_p"].as_f64().expect("should be f64");
        assert!((top_p - 0.9).abs() < 0.001, "expected ~0.9, got {top_p}");
    }

    #[test]
    fn stop_sequences_map_to_options_stop() {
        let mut req = minimal_request();
        req.stop_sequences = vec!["END".into(), "STOP".into()];
        let body = to_api_request(&req, "m", None);
        let stop = body["options"]["stop"].as_array().expect("should be array");
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0], "END");
        assert_eq!(stop[1], "STOP");
    }

    #[test]
    fn keep_alive_is_set() {
        let req = minimal_request();
        let body = to_api_request(&req, "m", Some("5m"));
        assert_eq!(body["keep_alive"], "5m");
    }

    #[test]
    fn keep_alive_not_set_when_none() {
        let req = minimal_request();
        let body = to_api_request(&req, "m", None);
        assert!(body.get("keep_alive").is_none() || body["keep_alive"].is_null());
    }

    #[test]
    fn tool_definitions_use_openai_format() {
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
        let body = to_api_request(&req, "m", None);
        let tools = body["tools"].as_array().expect("should be array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "search");
        assert_eq!(tools[0]["function"]["description"], "Search the web");
        assert!(tools[0]["function"]["parameters"].is_object());
    }

    #[test]
    fn extra_fields_forwarded_into_options() {
        let mut req = minimal_request();
        req.extra = Some(serde_json::json!({ "seed": 42, "num_ctx": 4096 }));
        let body = to_api_request(&req, "m", None);
        assert_eq!(body["options"]["seed"], 42);
        assert_eq!(body["options"]["num_ctx"], 4096);
    }

    #[test]
    fn no_stream_key_in_request() {
        let req = minimal_request();
        let body = to_api_request(&req, "m", None);
        assert!(body.get("stream").is_none());
    }

    // ─── Response parsing tests ────────────────────────────────

    #[test]
    fn parse_text_response() {
        let body = serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you today?"
            },
            "done": true,
            "done_reason": "stop",
            "eval_count": 10,
            "prompt_eval_count": 20,
        });
        let resp = from_api_response(&body).expect("should parse");
        assert_eq!(resp.model, "llama3.2");
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(resp.usage.input_tokens, 20);
        assert_eq!(resp.usage.output_tokens, 10);
        assert!(matches!(&resp.message.content[0], ContentBlock::Text(t) if t == "Hello! How can I help you today?"));
    }

    #[test]
    fn parse_tool_use_response() {
        let body = serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": "search",
                        "arguments": { "query": "rust" }
                    }
                }]
            },
            "done": true,
            "done_reason": "tool_calls",
            "eval_count": 15,
            "prompt_eval_count": 25,
        });
        let resp = from_api_response(&body).expect("should parse");
        assert_eq!(resp.stop_reason, StopReason::ToolUse);
        assert_eq!(resp.usage.input_tokens, 25);
        assert_eq!(resp.usage.output_tokens, 15);
        assert!(matches!(
            &resp.message.content[0],
            ContentBlock::ToolUse { name, input, id } if name == "search" && input["query"] == "rust" && id.starts_with("ollama_")
        ));
    }

    #[test]
    fn parse_response_with_no_usage() {
        let body = serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "Hi"
            },
            "done": true,
            "done_reason": "stop",
        });
        let resp = from_api_response(&body).expect("should parse");
        assert_eq!(resp.usage.input_tokens, 0);
        assert_eq!(resp.usage.output_tokens, 0);
    }

    #[test]
    fn parse_response_max_tokens_stop() {
        let body = serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "Truncated..."
            },
            "done": true,
            "done_reason": "length",
        });
        let resp = from_api_response(&body).expect("should parse");
        assert_eq!(resp.stop_reason, StopReason::MaxTokens);
    }

    #[test]
    fn synthesized_ids_are_unique() {
        let body = serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    { "function": { "name": "a", "arguments": {} } },
                    { "function": { "name": "b", "arguments": {} } },
                ]
            },
            "done": true,
            "done_reason": "tool_calls",
        });
        let resp = from_api_response(&body).expect("should parse");
        let ids: Vec<&str> = resp
            .message
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "synthesized IDs should be unique");
    }

    #[test]
    fn missing_model_returns_error() {
        let body = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "Hi"
            },
            "done": true,
        });
        let err = from_api_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    #[test]
    fn assistant_message_with_tool_calls_mapped() {
        let req = CompletionRequest {
            model: String::new(),
            messages: vec![Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tc_1".into(),
                    name: "search".into(),
                    input: serde_json::json!({"q": "test"}),
                }],
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
        };
        let body = to_api_request(&req, "m", None);
        let messages = body["messages"].as_array().expect("should be array");
        let tc = messages[0]["tool_calls"].as_array().expect("should have tool_calls");
        assert_eq!(tc.len(), 1);
        assert_eq!(tc[0]["function"]["name"], "search");
        assert_eq!(tc[0]["function"]["arguments"]["q"], "test");
    }
}
