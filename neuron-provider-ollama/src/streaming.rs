//! NDJSON streaming support for the Ollama Chat API.
//!
//! Parses the newline-delimited JSON stream produced by Ollama and maps events
//! to [`StreamEvent`] variants.
//!
//! Unlike Anthropic's SSE format, Ollama emits one JSON object per line:
//! ```text
//! {"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}
//! {"model":"llama3.2","message":{"role":"assistant","content":" world"},"done":false}
//! {"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","eval_count":10,"prompt_eval_count":20}
//! ```
//!
//! Reference: <https://github.com/ollama/ollama/blob/main/docs/api.md#generate-a-chat-completion>

use futures::{Stream, StreamExt};
use neuron_types::{
    ContentBlock, Message, Role, StreamError, StreamEvent, StreamHandle, TokenUsage,
};
use reqwest::Response;

/// Wrap an HTTP response body into a [`StreamHandle`] that emits [`StreamEvent`]s.
///
/// The response body is consumed as a byte stream. NDJSON lines are parsed and
/// mapped to stream events.
pub(crate) fn stream_completion(response: Response) -> StreamHandle {
    let byte_stream = response.bytes_stream();
    let event_stream = parse_ndjson_stream(byte_stream);
    StreamHandle {
        receiver: Box::pin(event_stream),
    }
}

/// Parse a raw byte stream into a stream of [`StreamEvent`]s from NDJSON.
///
/// This function handles buffering partial lines across byte chunks. Each
/// complete line is parsed as a JSON object and mapped to zero or more
/// [`StreamEvent`]s. The stream completes when the underlying byte stream
/// ends or an unrecoverable error is encountered.
fn parse_ndjson_stream(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = StreamEvent> + Send + 'static {
    async_stream::stream! {
        let mut state = NdjsonParserState::new();
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

            // Append chunk to line buffer and process complete lines
            line_buf.push_str(chunk_str);

            while let Some(newline_pos) = line_buf.find('\n') {
                let line = line_buf[..newline_pos].trim_end_matches('\r').to_string();
                line_buf.drain(..=newline_pos);

                if line.trim().is_empty() {
                    continue;
                }

                for event in state.process_line(&line) {
                    yield event;
                }
            }
        }

        // Process any remaining content in the buffer
        let remaining = line_buf.trim().to_string();
        if !remaining.is_empty() {
            for event in state.process_line(&remaining) {
                yield event;
            }
        }

        // Emit final assembled message
        if let Some(msg) = state.take_final_message() {
            yield StreamEvent::MessageComplete(msg);
        }
    }
}

/// Tracks in-progress streaming state across NDJSON lines.
struct NdjsonParserState {
    /// Accumulated text content across chunks.
    text_buf: String,
    /// In-progress tool calls (name, input JSON string).
    tool_calls: Vec<ToolCallInProgress>,
    /// The model name from the stream.
    model: String,
    /// Final usage statistics (from the done=true message).
    usage: Option<TokenUsage>,
}

/// An in-progress tool call being assembled during streaming.
struct ToolCallInProgress {
    /// Synthesized tool call ID.
    id: String,
    /// Tool function name.
    name: String,
    /// Tool function arguments (JSON value).
    arguments: serde_json::Value,
}

impl NdjsonParserState {
    fn new() -> Self {
        Self {
            text_buf: String::new(),
            tool_calls: Vec::new(),
            model: String::new(),
            usage: None,
        }
    }

    /// Process a single NDJSON line and return any events it produces.
    fn process_line(&mut self, line: &str) -> Vec<StreamEvent> {
        let json: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                return vec![StreamEvent::Error(StreamError::non_retryable(format!(
                    "JSON parse error in NDJSON: {e}"
                )))];
            }
        };

        // Capture model name
        if let Some(model) = json["model"].as_str() {
            self.model = model.to_string();
        }

        let done = json["done"].as_bool().unwrap_or(false);
        let mut events = Vec::new();

        // Extract text content from this chunk
        let content = json["message"]["content"].as_str().unwrap_or_default();
        if !content.is_empty() {
            self.text_buf.push_str(content);
            events.push(StreamEvent::TextDelta(content.to_string()));
        }

        // Extract tool calls (appear in the final message or in chunks)
        if let Some(tool_calls) = json["message"]["tool_calls"].as_array() {
            for tc in tool_calls {
                let function = &tc["function"];
                let name = function["name"].as_str().unwrap_or_default().to_string();
                let arguments = function["arguments"].clone();
                let id = format!("ollama_{}", uuid::Uuid::new_v4());

                events.push(StreamEvent::ToolUseStart {
                    id: id.clone(),
                    name: name.clone(),
                });

                // Emit the full input as a single delta
                let input_str = arguments.to_string();
                events.push(StreamEvent::ToolUseInputDelta {
                    id: id.clone(),
                    delta: input_str,
                });

                events.push(StreamEvent::ToolUseEnd { id: id.clone() });

                self.tool_calls.push(ToolCallInProgress {
                    id,
                    name,
                    arguments,
                });
            }
        }

        // If this is the final message, extract usage
        if done {
            let usage = TokenUsage {
                input_tokens: json["prompt_eval_count"].as_u64().unwrap_or(0) as usize,
                output_tokens: json["eval_count"].as_u64().unwrap_or(0) as usize,
                cache_read_tokens: None,
                cache_creation_tokens: None,
                reasoning_tokens: None,
                iterations: None,
            };
            self.usage = Some(usage.clone());
            events.push(StreamEvent::Usage(usage));
        }

        events
    }

    /// Assemble and return the final [`Message`] from buffered content.
    fn take_final_message(&mut self) -> Option<Message> {
        let mut content = Vec::new();

        if !self.text_buf.is_empty() {
            content.push(ContentBlock::Text(std::mem::take(&mut self.text_buf)));
        }

        for tc in &self.tool_calls {
            content.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.arguments.clone(),
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

    fn make_state() -> NdjsonParserState {
        NdjsonParserState::new()
    }

    #[test]
    fn parse_text_deltas() {
        let mut state = make_state();

        let events1 = state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}"#,
        );
        assert_eq!(events1.len(), 1);
        assert!(matches!(&events1[0], StreamEvent::TextDelta(t) if t == "Hello"));

        let events2 = state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":" world"},"done":false}"#,
        );
        assert_eq!(events2.len(), 1);
        assert!(matches!(&events2[0], StreamEvent::TextDelta(t) if t == " world"));

        assert_eq!(state.text_buf, "Hello world");
    }

    #[test]
    fn parse_final_message_with_usage() {
        let mut state = make_state();

        state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi"},"done":false}"#,
        );

        let events = state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","eval_count":10,"prompt_eval_count":20}"#,
        );

        let has_usage = events.iter().any(
            |e| matches!(e, StreamEvent::Usage(u) if u.input_tokens == 20 && u.output_tokens == 10),
        );
        assert!(has_usage, "expected Usage event");
    }

    #[test]
    fn parse_tool_calls_in_stream() {
        let mut state = make_state();

        let events = state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"search","arguments":{"query":"rust"}}}]},"done":true,"done_reason":"tool_calls","eval_count":15,"prompt_eval_count":25}"#,
        );

        let has_tool_start = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseStart { name, .. } if name == "search"));
        assert!(has_tool_start, "expected ToolUseStart event");

        let has_tool_input = events.iter().any(
            |e| matches!(e, StreamEvent::ToolUseInputDelta { delta, .. } if delta.contains("rust")),
        );
        assert!(has_tool_input, "expected ToolUseInputDelta event");

        let has_tool_end = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseEnd { .. }));
        assert!(has_tool_end, "expected ToolUseEnd event");

        let has_usage = events.iter().any(|e| matches!(e, StreamEvent::Usage(_)));
        assert!(has_usage, "expected Usage event");
    }

    #[test]
    fn take_final_message_assembles_text() {
        let mut state = make_state();
        state.text_buf = "Hello world".into();
        let msg = state.take_final_message().expect("should have message");
        assert_eq!(msg.role, Role::Assistant);
        assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello world"));
    }

    #[test]
    fn take_final_message_assembles_tool_calls() {
        let mut state = make_state();
        state.tool_calls.push(ToolCallInProgress {
            id: "ollama_test_1".into(),
            name: "search".into(),
            arguments: serde_json::json!({"q": "rust"}),
        });
        let msg = state.take_final_message().expect("should have message");
        assert!(matches!(
            &msg.content[0],
            ContentBlock::ToolUse { name, .. } if name == "search"
        ));
    }

    #[test]
    fn take_final_message_returns_none_when_empty() {
        let mut state = make_state();
        assert!(state.take_final_message().is_none());
    }

    #[test]
    fn invalid_json_yields_error_event() {
        let mut state = make_state();
        let events = state.process_line("not valid json");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], StreamEvent::Error(_)));
    }

    #[test]
    fn empty_content_does_not_emit_text_delta() {
        let mut state = make_state();
        let events = state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":false}"#,
        );
        // No TextDelta for empty content
        let text_deltas: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, StreamEvent::TextDelta(_)))
            .collect();
        assert!(text_deltas.is_empty());
    }

    #[test]
    fn model_name_captured() {
        let mut state = make_state();
        state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi"},"done":false}"#,
        );
        assert_eq!(state.model, "llama3.2");
    }

    #[test]
    fn multiple_chunks_accumulate_text() {
        let mut state = make_state();
        state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}"#,
        );
        state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":" "},"done":false}"#,
        );
        state.process_line(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"world"},"done":false}"#,
        );

        let msg = state.take_final_message().expect("should have message");
        assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello world"));
    }
}
