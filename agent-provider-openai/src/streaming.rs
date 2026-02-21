//! SSE streaming support for the OpenAI Chat Completions API.
//!
//! Parses the Server-Sent Events stream produced by OpenAI and maps events
//! to [`StreamEvent`] variants.
//!
//! Reference: <https://platform.openai.com/docs/api-reference/chat/streaming>

use std::collections::HashMap;

use agent_types::{ContentBlock, Message, Role, StreamEvent, StreamHandle, TokenUsage};
use futures::{Stream, StreamExt};
use reqwest::Response;

/// Wrap an HTTP response body into a [`StreamHandle`] that emits [`StreamEvent`]s.
///
/// The response body is consumed as a byte stream. SSE lines are parsed and
/// dispatched through the stream.
pub(crate) fn stream_completion(response: Response) -> StreamHandle {
    let byte_stream = response.bytes_stream();
    let event_stream = parse_sse_stream(byte_stream);
    StreamHandle {
        receiver: Box::pin(event_stream),
    }
}

/// Parse a raw byte stream into a stream of [`StreamEvent`]s.
///
/// OpenAI's SSE format is:
/// ```text
/// data: {"id":"...","choices":[{"delta":{"content":"text"}}]}
///
/// data: {"id":"...","choices":[{"delta":{"tool_calls":[...]}}]}
///
/// data: [DONE]
/// ```
fn parse_sse_stream(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = StreamEvent> + Send + 'static {
    async_stream::stream! {
        let mut state = SseParserState::new();
        let mut bytes_stream = std::pin::pin!(byte_stream);
        let mut line_buf = String::new();

        while let Some(chunk_result) = bytes_stream.next().await {
            let chunk = match chunk_result {
                Ok(b) => b,
                Err(e) => {
                    yield StreamEvent::Error(format!("stream read error: {e}"));
                    return;
                }
            };

            let chunk_str = match std::str::from_utf8(&chunk) {
                Ok(s) => s,
                Err(e) => {
                    yield StreamEvent::Error(format!("UTF-8 decode error: {e}"));
                    return;
                }
            };

            // Append chunk to our line buffer and process complete lines
            line_buf.push_str(chunk_str);

            // Split by newlines, keeping any incomplete line for the next chunk
            while let Some(newline_pos) = line_buf.find('\n') {
                let line = line_buf[..newline_pos].trim_end_matches('\r').to_string();
                line_buf.drain(..=newline_pos);

                // Process the line through the SSE parser
                for event in state.process_line(&line) {
                    yield event;
                }
            }
        }

        // Process any remaining content in the buffer
        if !line_buf.trim().is_empty() {
            for event in state.process_line(line_buf.trim()) {
                yield event;
            }
        }

        // Emit final message if we have one assembled
        if let Some(msg) = state.take_final_message() {
            yield StreamEvent::MessageComplete(msg);
        }
    }
}

/// Tracks in-progress streaming state across SSE data lines.
struct SseParserState {
    /// The current SSE data (from `data:` lines; may be multi-line).
    current_data: String,

    /// In-progress text being assembled.
    text_buf: String,
    /// Map from tool call index to in-progress tool use (id, name, input_json_buf).
    tool_uses: HashMap<usize, ToolUseInProgress>,

    /// Assembled usage statistics.
    usage: Option<TokenUsage>,
}

/// In-progress tool use block during streaming.
struct ToolUseInProgress {
    id: String,
    name: String,
    input_buf: String,
}

impl SseParserState {
    fn new() -> Self {
        Self {
            current_data: String::new(),
            text_buf: String::new(),
            tool_uses: HashMap::new(),
            usage: None,
        }
    }

    /// Process one SSE line and return any events it produces.
    fn process_line(&mut self, line: &str) -> Vec<StreamEvent> {
        if line.is_empty() {
            // Blank line: dispatch the accumulated data
            return self.dispatch_data();
        }

        if let Some(data) = line.strip_prefix("data: ") {
            if !self.current_data.is_empty() {
                self.current_data.push('\n');
            }
            self.current_data.push_str(data);
        }
        // Ignore event: lines, comment lines (starting with ':'), and other prefixes.
        // OpenAI doesn't use event: lines in their SSE format.

        vec![]
    }

    /// Dispatch the accumulated data, returning produced [`StreamEvent`]s.
    fn dispatch_data(&mut self) -> Vec<StreamEvent> {
        let data = std::mem::take(&mut self.current_data);

        if data.is_empty() {
            return vec![];
        }

        if data == "[DONE]" {
            return vec![];
        }

        let json: serde_json::Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(e) => {
                return vec![StreamEvent::Error(format!("JSON parse error in SSE: {e}"))];
            }
        };

        // Check for error object
        if let Some(error) = json.get("error") {
            let msg = error["message"]
                .as_str()
                .unwrap_or("unknown streaming error")
                .to_string();
            return vec![StreamEvent::Error(msg)];
        }

        let mut events = Vec::new();

        // Process choices[0].delta
        if let Some(choices) = json["choices"].as_array()
            && let Some(choice) = choices.first()
        {
            let delta = &choice["delta"];

            // Text content delta
            if let Some(content) = delta["content"].as_str()
                && !content.is_empty()
            {
                self.text_buf.push_str(content);
                events.push(StreamEvent::TextDelta(content.to_string()));
            }

            // Tool calls delta
            if let Some(tool_calls) = delta["tool_calls"].as_array() {
                for tc in tool_calls {
                    let index = tc["index"].as_u64().unwrap_or(0) as usize;

                    // If this delta has an id, it's a new tool call start
                    if let Some(id) = tc["id"].as_str() {
                        let name = tc["function"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let tool_id = id.to_string();
                        let tool_name = name.clone();
                        self.tool_uses.insert(
                            index,
                            ToolUseInProgress {
                                id: tool_id.clone(),
                                name,
                                input_buf: String::new(),
                            },
                        );
                        events.push(StreamEvent::ToolUseStart {
                            id: tool_id,
                            name: tool_name,
                        });
                    }

                    // Accumulate arguments fragment
                    if let Some(args) = tc["function"]["arguments"].as_str()
                        && !args.is_empty()
                        && let Some(tool) = self.tool_uses.get_mut(&index)
                    {
                        tool.input_buf.push_str(args);
                        events.push(StreamEvent::ToolUseInputDelta {
                            id: tool.id.clone(),
                            delta: args.to_string(),
                        });
                    }
                }
            }

            // Check for finish_reason — tool call end
            if let Some(finish_reason) = choice["finish_reason"].as_str()
                && (finish_reason == "tool_calls" || finish_reason == "stop")
            {
                // Emit ToolUseEnd for all in-progress tool calls
                for tool in self.tool_uses.values() {
                    events.push(StreamEvent::ToolUseEnd {
                        id: tool.id.clone(),
                    });
                }
            }
        }

        // Process usage (may be present in the final chunk)
        if let Some(usage_val) = json.get("usage")
            && usage_val.is_object()
        {
            let usage = TokenUsage {
                input_tokens: usage_val["prompt_tokens"].as_u64().unwrap_or(0) as usize,
                output_tokens: usage_val["completion_tokens"].as_u64().unwrap_or(0) as usize,
                cache_read_tokens: usage_val["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .map(|n| n as usize),
                cache_creation_tokens: None,
                reasoning_tokens: usage_val["completion_tokens_details"]["reasoning_tokens"]
                    .as_u64()
                    .map(|n| n as usize),
            };
            self.usage = Some(usage.clone());
            events.push(StreamEvent::Usage(usage));
        }

        events
    }

    /// Assemble and return the final [`Message`] from buffered content.
    pub(crate) fn take_final_message(&mut self) -> Option<Message> {
        let mut content = Vec::new();

        if !self.text_buf.is_empty() {
            content.push(ContentBlock::Text(std::mem::take(&mut self.text_buf)));
        }

        for tool in self.tool_uses.values() {
            let input: serde_json::Value =
                serde_json::from_str(&tool.input_buf).unwrap_or(serde_json::Value::Null);
            content.push(ContentBlock::ToolUse {
                id: tool.id.clone(),
                name: tool.name.clone(),
                input,
            });
        }

        if content.is_empty() {
            return None;
        }

        Some(Message {
            role: Role::Assistant,
            content,
        })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> SseParserState {
        SseParserState::new()
    }

    /// Helper: feed a multi-line SSE string to the parser and collect all events.
    fn feed_sse(state: &mut SseParserState, sse: &str) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        for line in sse.lines() {
            events.extend(state.process_line(line));
        }
        // Trigger any final dispatch (blank line at end of input)
        events.extend(state.process_line(""));
        events
    }

    #[test]
    fn parse_text_delta() {
        let mut state = make_state();
        let sse = "\
data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello \"},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"world\"},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}

data: [DONE]
";
        let events = feed_sse(&mut state, sse);
        let text_deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| {
                if let StreamEvent::TextDelta(t) = e {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text_deltas, vec!["Hello ", "world"]);
    }

    #[test]
    fn parse_tool_use_stream() {
        let mut state = make_state();
        let sse = "\
data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_abc123\",\"type\":\"function\",\"function\":{\"name\":\"search\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"q\\\"\"}}]},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":\\\"rust\\\"}\"}}]},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}

data: [DONE]
";
        let events = feed_sse(&mut state, sse);

        let has_tool_start = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseStart { id, name } if id == "call_abc123" && name == "search")
        });
        assert!(has_tool_start, "expected ToolUseStart event");

        let input_deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| {
                if let StreamEvent::ToolUseInputDelta { delta, .. } = e {
                    Some(delta.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(input_deltas.join(""), "{\"q\":\"rust\"}");

        let has_tool_end = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseEnd { id } if id == "call_abc123")
        });
        assert!(has_tool_end, "expected ToolUseEnd event");
    }

    #[test]
    fn parse_usage_in_stream() {
        let mut state = make_state();
        let sse = "\
data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12}}

data: [DONE]
";
        let events = feed_sse(&mut state, sse);
        let has_usage = events.iter().any(|e| {
            matches!(e, StreamEvent::Usage(u) if u.input_tokens == 10 && u.output_tokens == 2)
        });
        assert!(has_usage, "expected Usage event");
    }

    #[test]
    fn take_final_message_assembles_text() {
        let mut state = make_state();
        state.text_buf = "Hello world".into();
        let msg = state.take_final_message().unwrap();
        assert_eq!(msg.role, Role::Assistant);
        assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello world"));
    }

    #[test]
    fn take_final_message_assembles_tool_use() {
        let mut state = make_state();
        state.tool_uses.insert(
            0,
            ToolUseInProgress {
                id: "call_abc123".into(),
                name: "search".into(),
                input_buf: r#"{"q":"rust"}"#.into(),
            },
        );
        let msg = state.take_final_message().unwrap();
        assert!(matches!(
            &msg.content[0],
            ContentBlock::ToolUse { id, name, .. } if id == "call_abc123" && name == "search"
        ));
    }

    #[test]
    fn take_final_message_returns_none_when_empty() {
        let mut state = make_state();
        assert!(state.take_final_message().is_none());
    }

    #[test]
    fn done_sentinel_produces_no_events() {
        let mut state = make_state();
        let sse = "data: [DONE]\n";
        let events = feed_sse(&mut state, sse);
        assert!(events.is_empty());
    }

    #[test]
    fn error_object_produces_error_event() {
        let mut state = make_state();
        let sse =
            "data: {\"error\":{\"message\":\"Rate limit exceeded\",\"type\":\"rate_limit_error\"}}\n";
        let events = feed_sse(&mut state, sse);
        let has_error = events
            .iter()
            .any(|e| matches!(e, StreamEvent::Error(msg) if msg.contains("Rate limit")));
        assert!(has_error, "expected Error event");
    }

    #[test]
    fn multiple_tool_calls_tracked_by_index() {
        let mut state = make_state();
        let sse = "\
data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"search\",\"arguments\":\"\"}},{\"index\":1,\"id\":\"call_2\",\"type\":\"function\",\"function\":{\"name\":\"calc\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"q\\\":\\\"a\\\"}\"}}]},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"{\\\"x\\\":1}\"}}]},\"finish_reason\":null}]}

data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}

data: [DONE]
";
        let events = feed_sse(&mut state, sse);

        let starts: Vec<(&str, &str)> = events
            .iter()
            .filter_map(|e| {
                if let StreamEvent::ToolUseStart { id, name } = e {
                    Some((id.as_str(), name.as_str()))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(starts.len(), 2);
        assert!(starts.contains(&("call_1", "search")));
        assert!(starts.contains(&("call_2", "calc")));

        let msg = state.take_final_message().unwrap();
        assert_eq!(msg.content.len(), 2);
    }
}
