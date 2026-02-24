//! Configuration types for the agentic loop.

use neuron_types::{SystemPrompt, UsageLimits};

/// Configuration for the agentic loop.
#[derive(Debug, Clone)]
pub struct LoopConfig {
    /// The system prompt for the LLM provider.
    pub system_prompt: SystemPrompt,
    /// Maximum number of turns before the loop terminates.
    /// `None` means no limit.
    pub max_turns: Option<usize>,
    /// Whether to execute tool calls in parallel when multiple are returned.
    pub parallel_tool_execution: bool,
    /// Optional resource usage limits (token budgets, request/tool call caps).
    ///
    /// When set, the loop enforces limits at three check points:
    /// pre-request (request count), post-response (token totals),
    /// and pre-tool-call (tool call count).
    pub usage_limits: Option<UsageLimits>,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            system_prompt: SystemPrompt::Text(String::new()),
            max_turns: None,
            parallel_tool_execution: false,
            usage_limits: None,
        }
    }
}
