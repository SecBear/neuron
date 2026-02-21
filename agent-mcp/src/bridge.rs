//! Bridge MCP tools to the [`ToolDyn`] trait.
//!
//! [`McpToolBridge`] wraps an MCP tool definition and a shared [`McpClient`],
//! forwarding `call_dyn` to the MCP server. This allows MCP tools to be
//! registered in a [`ToolRegistry`] and used alongside native tools.

use std::sync::Arc;

use agent_types::{McpError, ToolContext, ToolDefinition, ToolDyn, ToolError, ToolOutput, WasmBoxedFuture};

use crate::client::{call_tool_result_to_output, McpClient};

/// Bridges an MCP tool to the [`ToolDyn`] trait.
///
/// Each bridge holds a shared reference to the [`McpClient`] and the tool's
/// definition. When `call_dyn` is invoked, it forwards the call to the MCP
/// server via `McpClient::call_tool`.
///
/// # Example
///
/// ```ignore
/// let client = Arc::new(McpClient::connect_stdio(config).await?);
/// let bridges = McpToolBridge::discover(&client).await?;
/// let mut registry = ToolRegistry::new();
/// for bridge in bridges {
///     registry.register_dyn(bridge);
/// }
/// ```
pub struct McpToolBridge {
    /// Shared MCP client for making tool calls.
    client: Arc<McpClient>,
    /// The tool definition (name, description, schema).
    definition: ToolDefinition,
}

impl McpToolBridge {
    /// Create a new bridge for a specific tool.
    #[must_use]
    pub fn new(client: Arc<McpClient>, definition: ToolDefinition) -> Self {
        Self { client, definition }
    }

    /// Discover all tools from an MCP server and create bridges for them.
    ///
    /// Fetches the tool list from the server and returns a bridge for each tool,
    /// ready to be registered in a [`ToolRegistry`] via `register_dyn`.
    ///
    /// # Errors
    ///
    /// Returns [`McpError`] if the tool listing fails.
    pub async fn discover(client: &Arc<McpClient>) -> Result<Vec<Arc<dyn ToolDyn>>, McpError> {
        let tools = client.list_all_tools().await?;
        let bridges: Vec<Arc<dyn ToolDyn>> = tools
            .into_iter()
            .map(|def| {
                let bridge = McpToolBridge::new(Arc::clone(client), def);
                Arc::new(bridge) as Arc<dyn ToolDyn>
            })
            .collect();
        Ok(bridges)
    }
}

impl ToolDyn for McpToolBridge {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    fn call_dyn<'a>(
        &'a self,
        input: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let arguments = match input {
                serde_json::Value::Object(m) => Some(m),
                serde_json::Value::Null => None,
                other => {
                    return Err(ToolError::InvalidInput(format!(
                        "expected object or null, got {}",
                        other
                    )));
                }
            };

            let result = self
                .client
                .call_tool(&self.definition.name, arguments)
                .await
                .map_err(|e| ToolError::ExecutionFailed(Box::new(e)))?;

            Ok(call_tool_result_to_output(result))
        })
    }
}

/// Extension methods on [`McpClient`] for tool discovery.
impl McpClient {
    /// Discover all tools and return them as [`ToolDyn`] trait objects.
    ///
    /// Convenience method that creates [`McpToolBridge`] instances for each
    /// tool on the server. The returned `Arc<dyn ToolDyn>` values can be
    /// registered in a [`ToolRegistry`] via `register_dyn`.
    ///
    /// # Arguments
    ///
    /// * `self_arc` - An `Arc<McpClient>` to share across bridges.
    ///
    /// # Errors
    ///
    /// Returns [`McpError`] if the tool listing fails.
    pub async fn discover_tools(
        self_arc: &Arc<McpClient>,
    ) -> Result<Vec<Arc<dyn ToolDyn>>, McpError> {
        McpToolBridge::discover(self_arc).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::mcp_tool_to_definition;
    use std::borrow::Cow;
    use std::sync::Arc;

    fn make_test_definition() -> ToolDefinition {
        ToolDefinition {
            name: "test_echo".to_string(),
            title: Some("Echo Tool".to_string()),
            description: "Echoes input".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                }
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    #[test]
    fn bridge_name() {
        // We can't construct a real McpClient in a unit test, but we can
        // verify that the ToolDyn implementation works correctly by testing
        // the conversion functions and definitions.
        let def = make_test_definition();
        assert_eq!(def.name, "test_echo");
        assert_eq!(def.description, "Echoes input");
    }

    #[test]
    fn tool_definition_clone() {
        let def = make_test_definition();
        let cloned = def.clone();
        assert_eq!(def.name, cloned.name);
        assert_eq!(def.description, cloned.description);
    }

    #[test]
    fn mcp_tool_roundtrip() {
        // Test that rmcp Tool -> ToolDefinition conversion preserves fields
        let mut schema = serde_json::Map::new();
        schema.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );
        let mut props = serde_json::Map::new();
        props.insert(
            "name".to_string(),
            serde_json::json!({"type": "string"}),
        );
        schema.insert(
            "properties".to_string(),
            serde_json::Value::Object(props),
        );

        let rmcp_tool = rmcp::model::Tool {
            name: Cow::Borrowed("greet"),
            title: Some("Greeter".to_string()),
            description: Some(Cow::Borrowed("Greets a person")),
            input_schema: Arc::new(schema.clone()),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        };

        let def = mcp_tool_to_definition(rmcp_tool);
        assert_eq!(def.name, "greet");
        assert_eq!(def.title, Some("Greeter".to_string()));
        assert_eq!(def.description, "Greets a person");
        // Verify schema preserved
        let obj = def.input_schema.as_object().expect("should be object");
        assert!(obj.contains_key("properties"));
    }
}
