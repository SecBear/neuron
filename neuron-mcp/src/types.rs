//! Types for MCP integration.

use serde::{Deserialize, Serialize};

/// A paginated list of items from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedList<T> {
    /// The items in this page.
    pub items: Vec<T>,
    /// Cursor for fetching the next page, if any.
    pub next_cursor: Option<String>,
}

/// An MCP resource returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// URI identifying the resource.
    pub uri: String,
    /// Human-readable name.
    pub name: String,
    /// Optional title.
    pub title: Option<String>,
    /// Optional description.
    pub description: Option<String>,
    /// MIME type of the resource content.
    pub mime_type: Option<String>,
}

/// Contents of a resource read from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceContents {
    /// URI of the resource.
    pub uri: String,
    /// MIME type of the content.
    pub mime_type: Option<String>,
    /// Text content (if text-based).
    pub text: Option<String>,
    /// Base64-encoded blob content (if binary).
    pub blob: Option<String>,
}

/// An MCP prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    /// The prompt name.
    pub name: String,
    /// Optional title.
    pub title: Option<String>,
    /// Optional description.
    pub description: Option<String>,
    /// Arguments the prompt accepts.
    pub arguments: Vec<McpPromptArgument>,
}

/// An argument for an MCP prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    /// Argument name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Whether this argument is required.
    pub required: Option<bool>,
}

/// A message returned from get_prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptMessage {
    /// The role (user or assistant).
    pub role: String,
    /// The message content.
    pub content: McpPromptContent,
}

/// Content of a prompt message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpPromptContent {
    /// Text content.
    Text(String),
    /// Image content.
    Image {
        /// Base64 data.
        data: String,
        /// MIME type.
        mime_type: String,
    },
    /// Embedded resource.
    Resource {
        /// The resource URI.
        uri: String,
        /// MIME type.
        mime_type: Option<String>,
        /// Text content.
        text: Option<String>,
    },
}

/// Result of a get_prompt call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptResult {
    /// Optional description of the prompt.
    pub description: Option<String>,
    /// The prompt messages.
    pub messages: Vec<McpPromptMessage>,
}
