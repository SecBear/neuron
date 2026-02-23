//! SSE streaming support for the Anthropic Messages API.
//!
//! Parses the Server-Sent Events stream produced by Anthropic and maps events
//! to [`StreamEvent`] variants.
//!
//! Reference: <https://docs.anthropic.com/en/api/messages-streaming>

use std::collections::HashMap;

use futures::{Stream, StreamExt};
use neuron_types::{
    ContentBlock, Message, Role, StreamError, StreamEvent, StreamHandle, TokenUsage,
};
use reqwest::Response;

/// Wrap an HTTP response body into a [`StreamHandle`] that emits [`StreamEvent`]s.
///
/// The response body is consumed as a byte stream. SSE lines are parsed and
/// dispatched through a `tokio::sync::mpsc` channel.
pub(crate) fn stream_completion(response: Response) -> StreamHandle {
    let byte_stream = response.bytes_stream();
    let event_stream = parse_sse_stream(byte_stream);
    StreamHandle {
        receiver: Box::pin(event_stream),
    }
}

/// Parse a raw byte stream into a stream of [`StreamEvent`]s.
///
/// This function drives all SSE parsing state internally. The stream completes
/// when the underlying byte stream ends or an unrecoverable error is encountered.
fn parse_sse_stream(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = StreamEvent> + Send + 'static {
    // We accumulate partial SSE lines across byte chunks using a string buffer.
    // The SSE format from Anthropic is:
    //
    //   event: content_block_delta
    //   data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}
    //
    //   (blank line terminates the event)
    //
    // We collect event+data pairs, then dispatch them.

    async_stream::stream! {
        let mut state = SseParserState::new();
        let mut bytes_stream = std::pin::pin!(byte_stream);
        let mut line_buf = String::new();

        while let Some(chunk_result) = bytes_stream.next().await {
            let chunk = match chunk_result {
                Ok(b) => b,
                Err(e) => {
                    yield StreamEvent::Error(StreamError::retryable(format!("stream read error: {e}")));
                    return;
                }
            };

            let chunk_str = match std::str::from_utf8(&chunk) {
                Ok(s) => s,
                Err(e) => {
                    yield StreamEvent::Error(StreamError::non_retryable(format!("UTF-8 decode error: {e}")));
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

        // Process any remaining complete message in the buffer
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

/// Tracks in-progress streaming state across SSE events.
struct SseParserState {
    /// The current SSE event type (from `event:` lines).
    current_event_type: Option<String>,
    /// The current SSE data (from `data:` lines; may be multi-line).
    current_data: String,

    /// In-progress text being assembled.
    text_buf: String,
    /// In-progress thinking content.
    thinking_buf: String,
    /// Map from block index to in-progress tool use (id, name, input_json_buf).
    tool_uses: HashMap<usize, ToolUseInProgress>,

    /// Assembled usage statistics (from `message_delta` event).
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
            current_event_type: None,
            current_data: String::new(),
            text_buf: String::new(),
            thinking_buf: String::new(),
            tool_uses: HashMap::new(),
            usage: None,
        }
    }

    /// Process one SSE line and return any events it produces.
    fn process_line(&mut self, line: &str) -> Vec<StreamEvent> {
        if line.is_empty() {
            // Blank line: dispatch the accumulated event
            return self.dispatch_event();
        }

        if let Some(event_type) = line.strip_prefix("event: ") {
            self.current_event_type = Some(event_type.trim().to_string());
        } else if let Some(data) = line.strip_prefix("data: ") {
            if !self.current_data.is_empty() {
                self.current_data.push('\n');
            }
            self.current_data.push_str(data.trim());
        }
        // Ignore comment lines (starting with ':') and other prefixes

        vec![]
    }

    /// Dispatch the accumulated event type + data, returning produced [`StreamEvent`]s.
    fn dispatch_event(&mut self) -> Vec<StreamEvent> {
        let event_type = match self.current_event_type.take() {
            Some(t) => t,
            None => {
                self.current_data.clear();
                return vec![];
            }
        };
        let data = std::mem::take(&mut self.current_data);

        if data == "[DONE]" || data.is_empty() {
            return vec![];
        }

        let json: serde_json::Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(e) => {
                return vec![StreamEvent::Error(StreamError::non_retryable(format!(
                    "JSON parse error in SSE: {e}"
                )))];
            }
        };

        match event_type.as_str() {
            "content_block_start" => self.handle_content_block_start(&json),
            "content_block_delta" => self.handle_content_block_delta(&json),
            "content_block_stop" => self.handle_content_block_stop(&json),
            "message_delta" => self.handle_message_delta(&json),
            "message_stop" => vec![], // final message emitted in take_final_message
            "message_start" => vec![], // We don't need message_start info
            "ping" => vec![],
            "error" => {
                let msg = json["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown streaming error")
                    .to_string();
                vec![StreamEvent::Error(StreamError::non_retryable(msg))]
            }
            _ => vec![], // Unknown event types are ignored
        }
    }

    fn handle_content_block_start(&mut self, json: &serde_json::Value) -> Vec<StreamEvent> {
        let index = json["index"].as_u64().unwrap_or(0) as usize;
        let block = &json["content_block"];
        let block_type = block["type"].as_str().unwrap_or("");

        match block_type {
            "text" => {
                // Nothing to emit; we'll emit TextDelta on each delta
                vec![]
            }
            "thinking" => vec![],
            "tool_use" => {
                let id = block["id"].as_str().unwrap_or("").to_string();
                let name = block["name"].as_str().unwrap_or("").to_string();
                let tool_id = id.clone();
                let tool_name = name.clone();
                self.tool_uses.insert(
                    index,
                    ToolUseInProgress {
                        id,
                        name,
                        input_buf: String::new(),
                    },
                );
                vec![StreamEvent::ToolUseStart {
                    id: tool_id,
                    name: tool_name,
                }]
            }
            _ => vec![],
        }
    }

    fn handle_content_block_delta(&mut self, json: &serde_json::Value) -> Vec<StreamEvent> {
        let index = json["index"].as_u64().unwrap_or(0) as usize;
        let delta = &json["delta"];
        let delta_type = delta["type"].as_str().unwrap_or("");

        match delta_type {
            "text_delta" => {
                let text = delta["text"].as_str().unwrap_or("").to_string();
                self.text_buf.push_str(&text);
                vec![StreamEvent::TextDelta(text)]
            }
            "thinking_delta" => {
                let thinking = delta["thinking"].as_str().unwrap_or("").to_string();
                self.thinking_buf.push_str(&thinking);
                vec![StreamEvent::ThinkingDelta(thinking)]
            }
            "signature_delta" => {
                let sig = delta["signature"].as_str().unwrap_or("").to_string();
                vec![StreamEvent::SignatureDelta(sig)]
            }
            "input_json_delta" => {
                let partial = delta["partial_json"].as_str().unwrap_or("").to_string();
                let tool_id = self
                    .tool_uses
                    .get_mut(&index)
                    .map(|t| {
                        t.input_buf.push_str(&partial);
                        t.id.clone()
                    })
                    .unwrap_or_default();
                vec![StreamEvent::ToolUseInputDelta {
                    id: tool_id,
                    delta: partial,
                }]
            }
            _ => vec![],
        }
    }

    fn handle_content_block_stop(&mut self, json: &serde_json::Value) -> Vec<StreamEvent> {
        let index = json["index"].as_u64().unwrap_or(0) as usize;
        if let Some(tool) = self.tool_uses.get(&index) {
            let id = tool.id.clone();
            vec![StreamEvent::ToolUseEnd { id }]
        } else {
            vec![]
        }
    }

    fn handle_message_delta(&mut self, json: &serde_json::Value) -> Vec<StreamEvent> {
        // Extract usage
        if let Some(usage_val) = json.get("usage") {
            let usage = TokenUsage {
                input_tokens: usage_val["input_tokens"].as_u64().unwrap_or(0) as usize,
                output_tokens: usage_val["output_tokens"].as_u64().unwrap_or(0) as usize,
                cache_read_tokens: usage_val["cache_read_input_tokens"]
                    .as_u64()
                    .map(|n| n as usize),
                cache_creation_tokens: usage_val["cache_creation_input_tokens"]
                    .as_u64()
                    .map(|n| n as usize),
                reasoning_tokens: None,
                iterations: None,
            };
            self.usage = Some(usage.clone());
            return vec![StreamEvent::Usage(usage)];
        }

        vec![]
    }

    /// Assemble and return the final [`Message`] from buffered content.
    pub(crate) fn take_final_message(&mut self) -> Option<Message> {
        let mut content = Vec::new();

        if !self.text_buf.is_empty() {
            content.push(ContentBlock::Text(std::mem::take(&mut self.text_buf)));
        }

        if !self.thinking_buf.is_empty() {
            content.push(ContentBlock::Thinking {
                thinking: std::mem::take(&mut self.thinking_buf),
                signature: String::new(),
            });
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
event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello \"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"world\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}
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
    fn parse_tool_use_events() {
        let mut state = make_state();
        let sse = "\
event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_01\",\"name\":\"search\",\"input\":{}}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"q\\\":\\\"rust\\\"}\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}
";
        let events = feed_sse(&mut state, sse);

        let has_tool_start = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseStart { id, name } if id == "toolu_01" && name == "search")
        });
        assert!(has_tool_start, "expected ToolUseStart event");

        let has_input_delta = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseInputDelta { id, .. } if id == "toolu_01"));
        assert!(has_input_delta, "expected ToolUseInputDelta event");

        let has_tool_end = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseEnd { id } if id == "toolu_01"));
        assert!(has_tool_end, "expected ToolUseEnd event");
    }

    #[test]
    fn parse_thinking_delta() {
        let mut state = make_state();
        let sse = "\
event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"I am thinking...\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}
";
        let events = feed_sse(&mut state, sse);
        let has_thinking = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ThinkingDelta(t) if t == "I am thinking..."));
        assert!(has_thinking, "expected ThinkingDelta event");
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
                id: "toolu_01".into(),
                name: "search".into(),
                input_buf: r#"{"q":"rust"}"#.into(),
            },
        );
        let msg = state.take_final_message().unwrap();
        assert!(matches!(
            &msg.content[0],
            ContentBlock::ToolUse { id, name, .. } if id == "toolu_01" && name == "search"
        ));
    }

    #[test]
    fn message_delta_emits_usage_event() {
        let mut state = make_state();
        let sse = "\
event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":42}}
";
        let events = feed_sse(&mut state, sse);
        let has_usage = events
            .iter()
            .any(|e| matches!(e, StreamEvent::Usage(u) if u.output_tokens == 42));
        assert!(has_usage, "expected Usage event");
    }

    // ─── Priority 3: Streaming edge cases ────────────────────────────────────

    #[test]
    fn parse_signature_delta() {
        let mut state = make_state();
        let sse = "\
event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Thinking...\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig_xyz789\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}
";
        let events = feed_sse(&mut state, sse);
        let has_signature = events
            .iter()
            .any(|e| matches!(e, StreamEvent::SignatureDelta(s) if s == "sig_xyz789"));
        assert!(has_signature, "expected SignatureDelta event");
    }

    #[test]
    fn parse_error_event() {
        let mut state = make_state();
        let sse = "\
event: error
data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}
";
        let events = feed_sse(&mut state, sse);
        let has_error = events.iter().any(
            |e| matches!(e, StreamEvent::Error(err) if err.message == "Overloaded" && !err.is_retryable),
        );
        assert!(has_error, "expected Error event with 'Overloaded' message");
    }

    #[test]
    fn error_event_with_missing_message_uses_default() {
        let mut state = make_state();
        let sse = "\
event: error
data: {\"type\":\"error\",\"error\":{\"type\":\"some_error\"}}
";
        let events = feed_sse(&mut state, sse);
        let has_error = events.iter().any(
            |e| matches!(e, StreamEvent::Error(err) if err.message == "unknown streaming error"),
        );
        assert!(
            has_error,
            "expected Error event with default 'unknown streaming error'"
        );
    }

    #[test]
    fn incomplete_sse_line_at_end_of_stream() {
        // Simulate receiving data that ends mid-event (no trailing newline to complete)
        let mut state = make_state();

        // First send a complete event
        let events1 = state.process_line("event: content_block_delta");
        assert!(events1.is_empty());

        let events2 = state.process_line(
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}",
        );
        assert!(events2.is_empty());

        // Dispatch on blank line
        let events3 = state.process_line("");
        let has_delta = events3
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta(t) if t == "Hello"));
        assert!(has_delta, "expected TextDelta from completed event");

        // Now simulate an incomplete line that arrives as the last data
        // This is the remaining content in line_buf when the stream ends
        let events4 = state.process_line("event: content_block_delta");
        assert!(events4.is_empty());
        let events5 = state.process_line(
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}",
        );
        assert!(events5.is_empty());

        // Simulate end-of-stream: process remaining buffer line + dispatch
        let events_final = state.process_line("");
        let has_world = events_final
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta(t) if t == " world"));
        assert!(has_world, "expected TextDelta from final buffer flush");

        // Verify the assembled message
        let msg = state.take_final_message().unwrap();
        assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello world"));
    }

    #[test]
    fn ping_event_produces_no_output() {
        let mut state = make_state();
        let sse = "\
event: ping
data: {}
";
        let events = feed_sse(&mut state, sse);
        assert!(events.is_empty(), "ping events should produce no output");
    }

    #[test]
    fn message_start_event_produces_no_output() {
        let mut state = make_state();
        let sse = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-20250514\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}
";
        let events = feed_sse(&mut state, sse);
        assert!(
            events.is_empty(),
            "message_start events should produce no output"
        );
    }

    #[test]
    fn message_stop_event_produces_no_output() {
        let mut state = make_state();
        let sse = "\
event: message_stop
data: {\"type\":\"message_stop\"}
";
        let events = feed_sse(&mut state, sse);
        assert!(
            events.is_empty(),
            "message_stop events should produce no output"
        );
    }

    #[test]
    fn unknown_event_type_ignored() {
        let mut state = make_state();
        let sse = "\
event: some_future_event
data: {\"type\":\"some_future_event\",\"foo\":\"bar\"}
";
        let events = feed_sse(&mut state, sse);
        assert!(events.is_empty(), "unknown event types should be ignored");
    }

    #[test]
    fn invalid_json_in_data_produces_error() {
        let mut state = make_state();
        let sse = "\
event: content_block_delta
data: {not valid json}
";
        let events = feed_sse(&mut state, sse);
        let has_error = events.iter().any(
            |e| matches!(e, StreamEvent::Error(err) if err.message.contains("JSON parse error")),
        );
        assert!(has_error, "expected Error event for invalid JSON");
    }

    #[test]
    fn done_sentinel_produces_no_events() {
        let mut state = make_state();
        let sse = "\
event: done
data: [DONE]
";
        let events = feed_sse(&mut state, sse);
        assert!(
            events.is_empty(),
            "[DONE] sentinel should produce no events"
        );
    }

    #[test]
    fn empty_data_produces_no_events() {
        let mut state = make_state();
        let sse = "\
event: content_block_delta
data:
";
        let events = feed_sse(&mut state, sse);
        assert!(events.is_empty(), "empty data should produce no events");
    }

    #[test]
    fn blank_line_with_no_accumulated_event_produces_nothing() {
        let mut state = make_state();
        let events = state.process_line("");
        assert!(
            events.is_empty(),
            "blank line with no pending event should produce nothing"
        );
    }

    #[test]
    fn take_final_message_with_thinking_content() {
        let mut state = make_state();
        state.thinking_buf = "Deep thoughts".into();
        let msg = state.take_final_message().unwrap();
        assert!(
            matches!(&msg.content[0], ContentBlock::Thinking { thinking, .. } if thinking == "Deep thoughts")
        );
    }

    #[test]
    fn take_final_message_returns_none_when_empty() {
        let mut state = make_state();
        assert!(state.take_final_message().is_none());
    }

    #[test]
    fn take_final_message_assembles_mixed_content() {
        let mut state = make_state();
        state.text_buf = "Final answer".into();
        state.thinking_buf = "My reasoning".into();
        state.tool_uses.insert(
            0,
            ToolUseInProgress {
                id: "toolu_01".into(),
                name: "calc".into(),
                input_buf: r#"{"expr":"2+2"}"#.into(),
            },
        );
        let msg = state.take_final_message().unwrap();
        assert_eq!(msg.content.len(), 3);
        assert!(
            msg.content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text(t) if t == "Final answer"))
        );
        assert!(msg.content.iter().any(
            |b| matches!(b, ContentBlock::Thinking { thinking, .. } if thinking == "My reasoning")
        ));
        assert!(
            msg.content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name == "calc"))
        );
    }

    #[test]
    fn message_delta_without_usage_produces_nothing() {
        let mut state = make_state();
        let sse = "\
event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}
";
        let events = feed_sse(&mut state, sse);
        // No usage field means no events
        assert!(
            events.is_empty(),
            "message_delta without usage should produce no events"
        );
    }

    #[test]
    fn content_block_stop_for_non_tool_block_produces_nothing() {
        let mut state = make_state();
        // Start a text block (not a tool use), then stop it
        let sse = "\
event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}
";
        let events = feed_sse(&mut state, sse);
        // Should have TextDelta but no ToolUseEnd
        let has_text = events
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta(t) if t == "Hi"));
        assert!(has_text);
        let has_tool_end = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseEnd { .. }));
        assert!(
            !has_tool_end,
            "content_block_stop on a text block should not emit ToolUseEnd"
        );
    }

    #[test]
    fn unknown_delta_type_produces_nothing() {
        let mut state = make_state();
        let sse = "\
event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"some_future_delta\",\"data\":\"abc\"}}
";
        let events = feed_sse(&mut state, sse);
        assert!(
            events.is_empty(),
            "unknown delta type should produce no events"
        );
    }

    #[test]
    fn unknown_content_block_start_type_produces_nothing() {
        let mut state = make_state();
        let sse = "\
event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"some_future_block\"}}
";
        let events = feed_sse(&mut state, sse);
        assert!(
            events.is_empty(),
            "unknown block type in content_block_start should produce no events"
        );
    }

    #[test]
    fn input_json_delta_without_tool_use_start() {
        let mut state = make_state();
        // Receive an input_json_delta for an index that was never started
        let sse = "\
event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":5,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"key\\\":\\\"val\\\"}\"}}
";
        let events = feed_sse(&mut state, sse);
        // Should still emit a ToolUseInputDelta, but with an empty id
        let has_delta = events.iter().any(
            |e| matches!(e, StreamEvent::ToolUseInputDelta { id, delta } if id.is_empty() && delta.contains("key")),
        );
        assert!(
            has_delta,
            "input_json_delta without matching start should emit with empty id"
        );
    }

    #[test]
    fn message_delta_with_cache_usage() {
        let mut state = make_state();
        let sse = "\
event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"input_tokens\":100,\"output_tokens\":50,\"cache_read_input_tokens\":80,\"cache_creation_input_tokens\":20}}
";
        let events = feed_sse(&mut state, sse);
        let usage_event = events.iter().find(|e| matches!(e, StreamEvent::Usage(_)));
        assert!(usage_event.is_some(), "expected Usage event");
        if let Some(StreamEvent::Usage(u)) = usage_event {
            assert_eq!(u.input_tokens, 100);
            assert_eq!(u.output_tokens, 50);
            assert_eq!(u.cache_read_tokens, Some(80));
            assert_eq!(u.cache_creation_tokens, Some(20));
        }
    }
}
