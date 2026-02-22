#![doc = include_str!("../README.md")]

// === Core blocks (always available) ===

/// Shared types and traits — the lingua franca of all blocks.
pub mod types {
    pub use agent_types::*;
}

/// Tool registry, middleware pipeline, and built-in middleware.
pub mod tool {
    pub use agent_tool::*;
}

/// Context management — token counting, compaction strategies, persistent context.
pub mod context {
    pub use agent_context::*;
}

/// The agentic while loop — composes provider + tools + context.
pub mod r#loop {
    pub use agent_loop::*;
}

// === Optional provider blocks ===

/// Anthropic Claude provider (Messages API, streaming, prompt caching).
#[cfg(feature = "anthropic")]
pub mod anthropic {
    pub use agent_provider_anthropic::*;
}

/// OpenAI provider (Chat Completions API, streaming, structured output).
#[cfg(feature = "openai")]
pub mod openai {
    pub use agent_provider_openai::*;
}

/// Ollama local provider (Chat API, NDJSON streaming).
#[cfg(feature = "ollama")]
pub mod ollama {
    pub use agent_provider_ollama::*;
}

// === Optional integration blocks ===

/// Model Context Protocol integration (wraps rmcp).
#[cfg(feature = "mcp")]
pub mod mcp {
    pub use agent_mcp::*;
}

/// Production runtime — sessions, sub-agents, guardrails, durability, sandboxing.
#[cfg(feature = "runtime")]
pub mod runtime {
    pub use agent_runtime::*;
}

// === Prelude — convenient imports for common usage ===

/// Common imports for working with agent blocks.
pub mod prelude {
    // Core types
    pub use agent_types::{
        CompletionRequest, CompletionResponse, ContentBlock, ContentItem, Message, Provider, Role,
        StopReason, SystemPrompt, TokenUsage, Tool, ToolContext, ToolDefinition, ToolDyn,
        ToolError, ToolOutput,
    };

    // Tool system
    pub use agent_tool::ToolRegistry;

    // Context strategies
    pub use agent_context::SlidingWindowStrategy;

    // The loop
    pub use agent_loop::{AgentLoop, AgentLoopBuilder, AgentResult, LoopConfig};
}
