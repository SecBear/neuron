//! MCP server that exposes a [`ToolRegistry`] via the MCP protocol.
//!
//! [`McpServer`] wraps a [`ToolRegistry`] and serves
//! its tools over stdio using the MCP protocol.

use std::borrow::Cow;
use std::sync::Arc;

use neuron_tool::ToolRegistry;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    ProtocolVersion, ServerCapabilities, ServerInfo, Tool as McpTool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::io::stdio;
use rmcp::{ErrorData, ServerHandler, ServiceExt};

use crate::error::McpError;

/// MCP server that exposes tools from a [`ToolRegistry`].
///
/// Call [`serve_stdio`](McpServer::serve_stdio) to start serving via stdin/stdout.
pub struct McpServer {
    /// The tool registry to expose.
    registry: Arc<ToolRegistry>,
    /// Server name for MCP identification.
    name: String,
    /// Server version for MCP identification.
    version: String,
}

impl McpServer {
    /// Create a new MCP server wrapping the given tool registry.
    pub fn new(
        registry: ToolRegistry,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            registry: Arc::new(registry),
            name: name.into(),
            version: version.into(),
        }
    }

    /// Serve the tools over stdio (stdin/stdout).
    ///
    /// This blocks until the client disconnects or an error occurs.
    ///
    /// # Errors
    ///
    /// Returns [`McpError::Connection`] if the transport setup or serving fails.
    pub async fn serve_stdio(self) -> Result<(), McpError> {
        let transport = stdio();
        let handler = McpServerHandler {
            registry: self.registry,
            name: self.name,
            version: self.version,
        };
        let service = handler
            .serve(transport)
            .await
            .map_err(|e| McpError::Connection(e.to_string()))?;
        service
            .waiting()
            .await
            .map_err(|e| McpError::Connection(e.to_string()))?;
        Ok(())
    }
}

/// Internal handler implementing [`ServerHandler`] for the MCP protocol.
struct McpServerHandler {
    /// The tool registry to expose.
    registry: Arc<ToolRegistry>,
    /// Server name.
    name: String,
    /// Server version.
    version: String,
}

impl ServerHandler for McpServerHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: self.name.clone(),
                version: self.version.clone(),
                ..Default::default()
            },
            instructions: None,
        }
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools: Vec<McpTool> = self
            .registry
            .iter()
            .map(|tool| {
                let schema = tool.input_schema();
                let schema_obj = schema.as_object().cloned().unwrap_or_default();

                McpTool {
                    name: Cow::Owned(tool.name().to_string()),
                    title: None,
                    description: Some(Cow::Owned(tool.description().to_string())),
                    input_schema: Arc::new(schema_obj),
                    output_schema: None,
                    annotations: None,
                    execution: None,
                    icons: None,
                    meta: None,
                }
            })
            .collect();

        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = &*request.name;
        let tool = self.registry.get(tool_name).ok_or_else(|| {
            ErrorData::invalid_params(format!("tool not found: {}", tool_name), None)
        })?;

        let input = match request.arguments {
            Some(map) => serde_json::Value::Object(map),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        match tool.call(input).await {
            Ok(result) => {
                let text =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_constructs() {
        let registry = ToolRegistry::new();
        let server = McpServer::new(registry, "test-server", "0.1.0");
        assert_eq!(server.name, "test-server");
        assert_eq!(server.version, "0.1.0");
    }
}
