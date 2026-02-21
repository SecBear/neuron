//! Streaming event types for incremental LLM responses.

use std::fmt;
use std::pin::Pin;

use futures::Stream;

use crate::types::{Message, TokenUsage};

/// Error information from a stream event.
#[derive(Debug, Clone)]
pub struct StreamError {
    /// Human-readable error message.
    pub message: String,
    /// Whether the error is retryable (e.g., rate limit, transient network).
    pub is_retryable: bool,
}

impl StreamError {
    /// Create a non-retryable error from a message string.
    #[must_use]
    pub fn non_retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_retryable: false,
        }
    }

    /// Create a retryable error from a message string.
    #[must_use]
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_retryable: true,
        }
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

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
    Error(StreamError),
}

/// Handle to a streaming completion response.
pub struct StreamHandle {
    /// The stream of events. Consume with `StreamExt::next()`.
    pub receiver: Pin<Box<dyn Stream<Item = StreamEvent> + Send>>,
}

impl fmt::Debug for StreamHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamHandle").finish_non_exhaustive()
    }
}
