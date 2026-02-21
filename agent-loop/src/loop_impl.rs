//! Core AgentLoop struct and run methods.

use agent_tool::ToolRegistry;
use agent_types::{
    ContextStrategy, Message, Provider, TokenUsage,
};

use crate::config::LoopConfig;

/// The result of a completed agent loop run.
#[derive(Debug)]
pub struct AgentResult {
    /// The final text response from the model.
    pub response: String,
    /// All messages in the conversation (including tool calls/results).
    pub messages: Vec<Message>,
    /// Cumulative token usage across all turns.
    pub usage: TokenUsage,
    /// Number of turns completed.
    pub turns: usize,
}

/// The agentic while loop: drives provider + tool + context interactions.
///
/// Generic over `P: Provider` (the LLM backend) and `C: ContextStrategy`
/// (the compaction strategy). Hooks and durability are optional.
pub struct AgentLoop<P: Provider, C: ContextStrategy> {
    provider: P,
    tools: ToolRegistry,
    context: C,
    config: LoopConfig,
    messages: Vec<Message>,
}

impl<P: Provider, C: ContextStrategy> AgentLoop<P, C> {
    /// Create a new `AgentLoop` with the given provider, tools, context strategy,
    /// and configuration.
    pub fn new(provider: P, tools: ToolRegistry, context: C, config: LoopConfig) -> Self {
        Self {
            provider,
            tools,
            context,
            config,
            messages: Vec::new(),
        }
    }

    /// Returns a reference to the current configuration.
    pub fn config(&self) -> &LoopConfig {
        &self.config
    }

    /// Returns a reference to the current messages.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Returns a mutable reference to the tool registry.
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.tools
    }
}
