//! Sub-agent spawning and management.
//!
//! A sub-agent runs a nested [`AgentLoop`] with a filtered subset of tools,
//! its own system prompt, and a max depth guard to prevent infinite nesting.

use std::collections::HashMap;

use agent_loop::{AgentLoop, AgentResult, LoopConfig};
use agent_tool::ToolRegistry;
use agent_types::{
    ContextStrategy, Message, Provider, SubAgentError, SystemPrompt, ToolContext,
};

/// Configuration for a sub-agent.
#[derive(Debug, Clone)]
pub struct SubAgentConfig {
    /// System prompt for the sub-agent's loop.
    pub system_prompt: SystemPrompt,
    /// Tool names to include (the sub-agent only sees these tools).
    pub tools: Vec<String>,
    /// Optional model override (currently informational; the provider decides).
    pub model: Option<String>,
    /// Maximum nesting depth (default 1). Prevents infinite recursion.
    pub max_depth: usize,
    /// Maximum turns for the sub-agent's loop.
    pub max_turns: Option<usize>,
}

impl SubAgentConfig {
    /// Create a new sub-agent configuration.
    #[must_use]
    pub fn new(system_prompt: SystemPrompt) -> Self {
        Self {
            system_prompt,
            tools: Vec::new(),
            model: None,
            max_depth: 1,
            max_turns: None,
        }
    }

    /// Set the allowed tools for this sub-agent.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    /// Set the maximum nesting depth.
    #[must_use]
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Set the maximum turns for the sub-agent's loop.
    #[must_use]
    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = Some(max_turns);
        self
    }
}

/// Manages named sub-agent configurations and spawns them.
///
/// Generic over nothing — the provider and context are passed at spawn time
/// because `Provider` and `ContextStrategy` use RPITIT and are not dyn-compatible.
pub struct SubAgentManager {
    configs: HashMap<String, SubAgentConfig>,
}

impl SubAgentManager {
    /// Create a new empty manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Register a named sub-agent configuration.
    pub fn register(&mut self, name: impl Into<String>, config: SubAgentConfig) {
        self.configs.insert(name.into(), config);
    }

    /// Get a registered sub-agent configuration by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SubAgentConfig> {
        self.configs.get(name)
    }

    /// Spawn a sub-agent by name, running it to completion.
    ///
    /// Filters the parent's tool registry to only include tools listed in the
    /// sub-agent's configuration. Takes the provider and context strategy as
    /// generics because they use RPITIT.
    ///
    /// # Errors
    ///
    /// Returns `SubAgentError::NotFound` if the name is not registered.
    /// Returns `SubAgentError::MaxDepthExceeded` if `current_depth >= config.max_depth`.
    /// Returns `SubAgentError::Loop` if the sub-agent's loop fails.
    #[must_use = "this returns a Result that should be handled"]
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn<P: Provider, C: ContextStrategy>(
        &self,
        name: &str,
        provider: P,
        context: C,
        parent_tools: &ToolRegistry,
        user_message: Message,
        tool_ctx: &ToolContext,
        current_depth: usize,
    ) -> Result<AgentResult, SubAgentError> {
        let config = self
            .configs
            .get(name)
            .ok_or_else(|| SubAgentError::NotFound(name.to_string()))?;

        if current_depth >= config.max_depth {
            return Err(SubAgentError::MaxDepthExceeded(config.max_depth));
        }

        // Build filtered tool registry
        let mut filtered_tools = ToolRegistry::new();
        for tool_name in &config.tools {
            if let Some(tool) = parent_tools.get(tool_name) {
                filtered_tools.register_dyn(tool);
            }
        }

        let loop_config = LoopConfig {
            system_prompt: config.system_prompt.clone(),
            max_turns: config.max_turns,
            parallel_tool_execution: false,
        };

        let mut agent_loop = AgentLoop::new(provider, filtered_tools, context, loop_config);
        let result = agent_loop.run(user_message, tool_ctx).await?;
        Ok(result)
    }

    /// Spawn multiple sub-agents in parallel.
    ///
    /// Each entry is `(name, provider, context, user_message)`. All share the
    /// same parent tools and tool context. Returns results in the same order
    /// as the input.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered (all tasks are awaited).
    #[must_use = "this returns a Result that should be handled"]
    pub async fn spawn_parallel<P, C>(
        &self,
        tasks: Vec<(String, P, C, Message)>,
        parent_tools: &ToolRegistry,
        tool_ctx: &ToolContext,
        current_depth: usize,
    ) -> Vec<Result<AgentResult, SubAgentError>>
    where
        P: Provider,
        C: ContextStrategy,
    {
        // We cannot use join_all with tokio::spawn because Provider/ContextStrategy
        // are not 'static in general. Instead, we run them sequentially for correctness.
        // For true parallelism, callers can use tokio::spawn with 'static bounds.
        let mut results = Vec::with_capacity(tasks.len());
        for (name, provider, context, message) in tasks {
            let result = self
                .spawn(
                    &name,
                    provider,
                    context,
                    parent_tools,
                    message,
                    tool_ctx,
                    current_depth,
                )
                .await;
            results.push(result);
        }
        results
    }
}

impl Default for SubAgentManager {
    fn default() -> Self {
        Self::new()
    }
}
