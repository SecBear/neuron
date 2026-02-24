#![doc = include_str!("../README.md")]

// === Core blocks (always available) ===

/// Shared types and traits — the lingua franca of all blocks.
pub mod types {
    pub use neuron_types::*;
}

/// Tool registry, middleware pipeline, and built-in middleware.
pub mod tool {
    pub use neuron_tool::*;
}

/// Context management — token counting, compaction strategies, persistent context.
pub mod context {
    pub use neuron_context::*;
}

/// The agentic while loop — composes provider + tools + context.
pub mod r#loop {
    pub use neuron_loop::*;
}

// === Optional provider blocks ===

/// Anthropic Claude provider (Messages API, streaming, prompt caching).
#[cfg(feature = "anthropic")]
pub mod anthropic {
    pub use neuron_provider_anthropic::*;
}

/// OpenAI provider (Chat Completions API, streaming, structured output).
#[cfg(feature = "openai")]
pub mod openai {
    pub use neuron_provider_openai::*;
}

/// Ollama local provider (Chat API, NDJSON streaming).
#[cfg(feature = "ollama")]
pub mod ollama {
    pub use neuron_provider_ollama::*;
}

// === Optional integration blocks ===

/// Model Context Protocol integration (wraps rmcp).
#[cfg(feature = "mcp")]
pub mod mcp {
    pub use neuron_mcp::*;
}

/// Production runtime — sessions, sub-agents, guardrails, durability, sandboxing.
#[cfg(feature = "runtime")]
pub mod runtime {
    pub use neuron_runtime::*;
}

/// OpenTelemetry instrumentation with GenAI semantic conventions.
#[cfg(feature = "otel")]
pub mod otel {
    pub use neuron_otel::*;
}

// === Prelude — convenient imports for common usage ===

/// Common imports for working with agent blocks.
pub mod prelude {
    // Core types
    pub use neuron_types::{
        CompletionRequest, CompletionResponse, ContentBlock, ContentItem, Message, Provider, Role,
        StopReason, SystemPrompt, TokenUsage, Tool, ToolContext, ToolDefinition, ToolDyn,
        ToolError, ToolOutput,
    };

    // Tool system
    pub use neuron_tool::ToolRegistry;

    // Context strategies
    pub use neuron_context::SlidingWindowStrategy;

    // The loop
    pub use neuron_loop::{AgentLoop, AgentLoopBuilder, AgentResult, LoopConfig};
}
