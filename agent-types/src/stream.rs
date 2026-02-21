//! Streaming event types for incremental LLM responses.

use std::pin::Pin;

use futures::Stream;

use crate::types::{Message, TokenUsage};

/// An event emitted during streaming completion.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Incremental text content.
    TextDelta(String),
    /// Incremental thinking/reasoning content.
    ThinkingDelta(String),
    /// Incremental signature content for thinking verification.
    SignatureDelta(String),
    /// A tool use block has started.
    ToolUseStart {
        /// Tool call identifier.
        id: String,
        /// Tool name.
        name: String,
    },
    /// Incremental tool input JSON.
    ToolUseInputDelta {
        /// Tool call identifier (matches `ToolUseStart.id`).
        id: String,
        /// JSON fragment.
        delta: String,
    },
    /// A tool use block has ended.
    ToolUseEnd {
        /// Tool call identifier.
        id: String,
    },
    /// The complete assembled message (sent at the end of the stream).
    MessageComplete(Message),
    /// Token usage statistics for the stream.
    Usage(TokenUsage),
    /// An error occurred during streaming.
    Error(String),
}

/// Handle to a streaming completion response.
pub struct StreamHandle {
    /// The stream of events. Consume with `StreamExt::next()`.
    pub receiver: Pin<Box<dyn Stream<Item = StreamEvent> + Send>>,
}
