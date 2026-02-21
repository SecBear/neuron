//! Core message and request/response types.

use serde::{Deserialize, Serialize};

/// The role of a message participant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// A human user.
    User,
    /// An AI assistant.
    Assistant,
    /// A system message.
    System,
}

/// A content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    /// Plain text content.
    Text(String),
    /// Extended thinking from reasoning models.
    Thinking {
        /// The thinking text.
        thinking: String,
        /// Cryptographic signature for verification.
        signature: String,
    },
    /// Redacted thinking (not visible to user).
    RedactedThinking {
        /// Opaque data blob.
        data: String,
    },
    /// A tool invocation request from the assistant.
    ToolUse {
        /// Unique identifier for this tool call.
        id: String,
        /// Name of the tool to invoke.
        name: String,
        /// JSON input arguments.
        input: serde_json::Value,
    },
    /// Result of a tool invocation.
    ToolResult {
        /// References the `id` from the corresponding `ToolUse`.
        tool_use_id: String,
        /// Content items in the result.
        content: Vec<ContentItem>,
        /// Whether this result represents an error.
        is_error: bool,
    },
    /// An image content block.
    Image {
        /// The image source.
        source: ImageSource,
    },
    /// A document content block.
    Document {
        /// The document source.
        source: DocumentSource,
    },
}

/// A content item within a tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentItem {
    /// Plain text content.
    Text(String),
    /// An image.
    Image {
        /// The image source.
        source: ImageSource,
    },
}

/// Source of an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSource {
    /// Base64-encoded image data.
    Base64 {
        /// MIME type (e.g. "image/png").
        media_type: String,
        /// Base64-encoded data.
        data: String,
    },
    /// URL to an image.
    Url {
        /// The image URL.
        url: String,
    },
}

/// Source of a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentSource {
    /// Base64-encoded PDF.
    Base64Pdf {
        /// Base64-encoded PDF data.
        data: String,
    },
    /// Plain text document.
    PlainText {
        /// The text content.
        data: String,
    },
    /// URL to a document.
    Url {
        /// The document URL.
        url: String,
    },
}

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The role of the message author.
    pub role: Role,
    /// The content blocks of this message.
    pub content: Vec<ContentBlock>,
}
