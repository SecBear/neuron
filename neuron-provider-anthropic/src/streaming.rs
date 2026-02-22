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
}
