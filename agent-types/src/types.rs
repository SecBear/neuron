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

// --- Completion request/response types ---

/// System prompt configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemPrompt {
    /// A simple text system prompt.
    Text(String),
    /// Structured system prompt blocks with optional cache control.
    Blocks(Vec<SystemBlock>),
}

/// A block within a structured system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemBlock {
    /// The text content of this block.
    pub text: String,
    /// Optional cache control for this block.
    pub cache_control: Option<CacheControl>,
}

/// Cache control configuration for prompt caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    /// Time-to-live for the cached content.
    pub ttl: Option<CacheTtl>,
}

/// Cache time-to-live options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheTtl {
    /// Cache for 5 minutes.
    FiveMinutes,
    /// Cache for 1 hour.
    OneHour,
}

/// Tool selection strategy for the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChoice {
    /// Model decides whether to use tools.
    Auto,
    /// Model must not use tools.
    None,
    /// Model must use at least one tool.
    Required,
    /// Model must use the specified tool.
    Specific {
        /// Name of the required tool.
        name: String,
    },
}

/// Response format configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseFormat {
    /// Plain text response.
    Text,
    /// JSON object response.
    JsonObject,
    /// Structured JSON response with schema validation.
    JsonSchema {
        /// Name of the schema.
        name: String,
        /// The JSON Schema.
        schema: serde_json::Value,
        /// Whether to enforce strict schema adherence.
        strict: bool,
    },
}

/// Extended thinking configuration for reasoning models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThinkingConfig {
    /// Enable thinking with a token budget.
    Enabled {
        /// Maximum tokens for thinking.
        budget_tokens: usize,
    },
    /// Disable thinking.
    Disabled,
    /// Let the model decide.
    Adaptive,
}

/// Reasoning effort level for reasoning models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReasoningEffort {
    /// No reasoning.
    None,
    /// Minimal reasoning.
    Low,
    /// Moderate reasoning.
    Medium,
    /// Maximum reasoning.
    High,
}

/// A completion request to an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// The model identifier.
    pub model: String,
    /// The conversation messages.
    pub messages: Vec<Message>,
    /// Optional system prompt.
    pub system: Option<SystemPrompt>,
    /// Tool definitions available to the model.
    pub tools: Vec<ToolDefinition>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<usize>,
    /// Sampling temperature (0.0 to 1.0).
    pub temperature: Option<f32>,
    /// Nucleus sampling parameter.
    pub top_p: Option<f32>,
    /// Sequences that cause generation to stop.
    pub stop_sequences: Vec<String>,
    /// Tool selection strategy.
    pub tool_choice: Option<ToolChoice>,
    /// Response format constraint.
    pub response_format: Option<ResponseFormat>,
    /// Extended thinking configuration.
    pub thinking: Option<ThinkingConfig>,
    /// Reasoning effort level.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Provider-specific extra fields forwarded verbatim.
    pub extra: Option<serde_json::Value>,
}

/// A completion response from an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Provider-assigned message ID.
    pub id: String,
    /// The model that generated this response.
    pub model: String,
    /// The response message.
    pub message: Message,
    /// Token usage statistics.
    pub usage: TokenUsage,
    /// Why the model stopped generating.
    pub stop_reason: StopReason,
}

/// Reason the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// Model reached a natural end.
    EndTurn,
    /// Model wants to use a tool.
    ToolUse,
    /// Hit the max token limit.
    MaxTokens,
    /// Hit a stop sequence.
    StopSequence,
    /// Content was filtered.
    ContentFilter,
}

/// Token usage statistics for a completion.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens in the input/prompt.
    pub input_tokens: usize,
    /// Tokens in the output/completion.
    pub output_tokens: usize,
    /// Tokens read from cache.
    pub cache_read_tokens: Option<usize>,
    /// Tokens written to cache.
    pub cache_creation_tokens: Option<usize>,
    /// Tokens used for reasoning/thinking.
    pub reasoning_tokens: Option<usize>,
}

// --- Tool definition types (needed by CompletionRequest) ---

/// Definition of a tool available to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// The tool name (unique identifier).
    pub name: String,
    /// Optional human-readable title.
    pub title: Option<String>,
    /// Description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
    /// Optional JSON Schema for the tool's output.
    pub output_schema: Option<serde_json::Value>,
    /// Optional behavioral annotations (MCP spec).
    pub annotations: Option<ToolAnnotations>,
    /// Optional cache control for this tool definition.
    pub cache_control: Option<CacheControl>,
}

/// Behavioral annotations for a tool (from MCP spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAnnotations {
    /// Whether the tool only reads data.
    pub read_only_hint: Option<bool>,
    /// Whether the tool performs destructive operations.
    pub destructive_hint: Option<bool>,
    /// Whether repeated calls with same args produce same result.
    pub idempotent_hint: Option<bool>,
    /// Whether the tool interacts with external systems.
    pub open_world_hint: Option<bool>,
}
