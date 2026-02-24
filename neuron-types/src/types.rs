//! Core message and request/response types.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::wasm::{WasmCompatSend, WasmCompatSync};

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
    /// Server-side context compaction summary.
    Compaction {
        /// The compacted context summary.
        content: String,
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

impl Message {
    /// Create a user message with a single text content block.
    ///
    /// # Example
    ///
    /// ```
    /// use neuron_types::Message;
    /// let msg = Message::user("What is Rust?");
    /// ```
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text(text.into())],
        }
    }

    /// Create an assistant message with a single text content block.
    ///
    /// # Example
    ///
    /// ```
    /// use neuron_types::Message;
    /// let msg = Message::assistant("Rust is a systems programming language.");
    /// ```
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::Text(text.into())],
        }
    }

    /// Create a system message with a single text content block.
    ///
    /// # Example
    ///
    /// ```
    /// use neuron_types::Message;
    /// let msg = Message::system("You are a helpful assistant.");
    /// ```
    #[must_use]
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: vec![ContentBlock::Text(text.into())],
        }
    }
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

/// Server-side context management configuration.
///
/// Instructs the provider to manage context window size automatically,
/// e.g. by compacting conversation history when it grows too large.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ContextManagement {
    /// The context edits to apply.
    pub edits: Vec<ContextEdit>,
}

/// A context editing operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContextEdit {
    /// Compact conversation history using the named strategy.
    Compact {
        /// Strategy identifier (e.g. `"compact_20260112"`).
        strategy: String,
    },
}

/// Per-iteration token usage breakdown (returned during server-side compaction).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UsageIteration {
    /// Tokens in the input for this iteration.
    pub input_tokens: usize,
    /// Tokens in the output for this iteration.
    pub output_tokens: usize,
    /// Tokens read from cache in this iteration.
    pub cache_read_tokens: Option<usize>,
    /// Tokens written to cache in this iteration.
    pub cache_creation_tokens: Option<usize>,
}

// --- Embedding types ---

/// A request to an embedding model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmbeddingRequest {
    /// The embedding model to use (e.g. `"text-embedding-3-small"`).
    pub model: String,
    /// The text inputs to embed.
    pub input: Vec<String>,
    /// Optional number of dimensions for the output embeddings.
    pub dimensions: Option<usize>,
    /// Provider-specific extra fields forwarded verbatim.
    pub extra: HashMap<String, serde_json::Value>,
}

/// Response from an embedding request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// The embedding vectors, one per input string.
    pub embeddings: Vec<Vec<f32>>,
    /// The model that generated the embeddings.
    pub model: String,
    /// Token usage statistics.
    pub usage: EmbeddingUsage,
}

/// Token usage statistics for an embedding request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    /// Number of tokens in the input.
    pub prompt_tokens: usize,
    /// Total tokens consumed.
    pub total_tokens: usize,
}

/// A completion request to an LLM provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Server-side context management configuration.
    pub context_management: Option<ContextManagement>,
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
    /// Server paused to compact context.
    Compaction,
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
    /// Per-iteration token breakdown (for server-side compaction).
    pub iterations: Option<Vec<UsageIteration>>,
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

/// Output from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Human-readable content items.
    pub content: Vec<ContentItem>,
    /// Optional structured JSON output for programmatic consumption.
    pub structured_content: Option<serde_json::Value>,
    /// Whether this output represents an error.
    pub is_error: bool,
}

/// Runtime context provided to tools during execution.
pub struct ToolContext {
    /// Current working directory.
    pub cwd: PathBuf,
    /// Session identifier.
    pub session_id: String,
    /// Environment variables available to the tool.
    pub environment: HashMap<String, String>,
    /// Token for cooperative cancellation.
    pub cancellation_token: CancellationToken,
    /// Optional progress reporter for long-running tools.
    pub progress_reporter: Option<Arc<dyn ProgressReporter>>,
}

impl Default for ToolContext {
    /// Creates a ToolContext with sensible defaults:
    /// - `cwd`: current directory (falls back to `/tmp` if unavailable)
    /// - `session_id`: empty string
    /// - `environment`: empty
    /// - `cancellation_token`: new token
    /// - `progress_reporter`: None
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp")),
            session_id: String::new(),
            environment: HashMap::new(),
            cancellation_token: CancellationToken::new(),
            progress_reporter: None,
        }
    }
}

/// Resource usage limits for the agentic loop.
///
/// Controls how many requests, tool calls, and tokens the loop may consume
/// before terminating with `LoopError::UsageLimitExceeded`.
/// All limits are optional — only set limits are enforced.
///
/// # Example
///
/// ```
/// use neuron_types::UsageLimits;
///
/// let limits = UsageLimits::default()
///     .with_request_limit(50)
///     .with_total_tokens_limit(100_000);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageLimits {
    /// Maximum number of LLM requests (provider calls) allowed.
    pub request_limit: Option<usize>,
    /// Maximum number of tool calls allowed across all turns.
    pub tool_calls_limit: Option<usize>,
    /// Maximum input tokens allowed across all turns.
    pub input_tokens_limit: Option<usize>,
    /// Maximum output tokens allowed across all turns.
    pub output_tokens_limit: Option<usize>,
    /// Maximum total tokens (input + output) allowed across all turns.
    pub total_tokens_limit: Option<usize>,
}

impl UsageLimits {
    /// Set the maximum number of LLM requests.
    #[must_use]
    pub fn with_request_limit(mut self, limit: usize) -> Self {
        self.request_limit = Some(limit);
        self
    }

    /// Set the maximum number of tool calls.
    #[must_use]
    pub fn with_tool_calls_limit(mut self, limit: usize) -> Self {
        self.tool_calls_limit = Some(limit);
        self
    }

    /// Set the maximum input tokens.
    #[must_use]
    pub fn with_input_tokens_limit(mut self, limit: usize) -> Self {
        self.input_tokens_limit = Some(limit);
        self
    }

    /// Set the maximum output tokens.
    #[must_use]
    pub fn with_output_tokens_limit(mut self, limit: usize) -> Self {
        self.output_tokens_limit = Some(limit);
        self
    }

    /// Set the maximum total tokens (input + output).
    #[must_use]
    pub fn with_total_tokens_limit(mut self, limit: usize) -> Self {
        self.total_tokens_limit = Some(limit);
        self
    }
}

/// Reports progress for long-running tool operations.
pub trait ProgressReporter: WasmCompatSend + WasmCompatSync {
    /// Report progress.
    ///
    /// # Arguments
    /// * `progress` - Current progress value.
    /// * `total` - Optional total value (for percentage calculation).
    /// * `message` - Optional status message.
    fn report(&self, progress: f64, total: Option<f64>, message: Option<&str>);
}

impl From<String> for SystemPrompt {
    fn from(s: String) -> Self {
        SystemPrompt::Text(s)
    }
}

impl From<&str> for SystemPrompt {
    fn from(s: &str) -> Self {
        SystemPrompt::Text(s.to_string())
    }
}
