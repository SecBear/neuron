//! Tool registry: register, lookup, and execute tools.

use std::collections::HashMap;
use std::sync::Arc;

use agent_types::{Tool, ToolContext, ToolDefinition, ToolDyn, ToolError, ToolOutput};

use crate::middleware::{Next, ToolCall, ToolMiddleware};

/// Registry of tools with optional middleware pipelines.
///
/// Tools are stored as type-erased [`ToolDyn`] trait objects.
/// Middleware can be added globally (applies to all tools) or per-tool.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolDyn>>,
    global_middleware: Vec<Arc<dyn ToolMiddleware>>,
    tool_middleware: HashMap<String, Vec<Arc<dyn ToolMiddleware>>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            global_middleware: Vec::new(),
            tool_middleware: HashMap::new(),
        }
    }

    /// Register a strongly-typed tool (auto-erased to `ToolDyn`).
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = T::NAME.to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    /// Register a pre-erased tool.
    pub fn register_dyn(&mut self, tool: Arc<dyn ToolDyn>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolDyn>> {
        self.tools.get(name).cloned()
    }

    /// Get definitions for all registered tools.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Add global middleware (applies to all tool executions).
    pub fn add_middleware(&mut self, m: impl ToolMiddleware + 'static) -> &mut Self {
        self.global_middleware.push(Arc::new(m));
        self
    }

    /// Add middleware that only applies to a specific tool.
    pub fn add_tool_middleware(
        &mut self,
        tool_name: &str,
        m: impl ToolMiddleware + 'static,
    ) -> &mut Self {
        self.tool_middleware
            .entry(tool_name.to_string())
            .or_default()
            .push(Arc::new(m));
        self
    }

    /// Execute a tool by name, running it through the middleware chain.
    ///
    /// Middleware order: global middleware first, then per-tool middleware,
    /// then the actual tool.
    pub async fn execute(
        &self,
        name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;

        let call = ToolCall {
            id: String::new(),
            name: name.to_string(),
            input,
        };

        // Build combined middleware chain: global + per-tool
        let mut chain: Vec<Arc<dyn ToolMiddleware>> = self.global_middleware.clone();
        if let Some(per_tool) = self.tool_middleware.get(name) {
            chain.extend(per_tool.iter().cloned());
        }

        let next = Next::new(tool.as_ref(), &chain);
        next.run(&call, ctx).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
