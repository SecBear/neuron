//! MCP server: expose a [`ToolRegistry`] as an MCP server.
//!
//! [`McpServer`] wraps a [`ToolRegistry`] and implements the rmcp [`ServerHandler`]
//! trait, allowing registered tools to be accessed by MCP clients.

use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ErrorData, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool as RmcpTool, ToolAnnotations as RmcpToolAnnotations,
    ToolsCapability,
};
use rmcp::service::{RequestContext, RoleServer};

use agent_types::{McpError as AgentMcpError, ToolContext, ToolDefinition};
use agent_tool::ToolRegistry;

/// MCP server that exposes a [`ToolRegistry`] via the MCP protocol.
///
/// Tools registered in the registry become available to MCP clients.
/// The server handles `tools/list` and `tools/call` requests by delegating
/// to the underlying [`ToolRegistry`].
///
/// # Example
///
/// ```ignore
/// use agent_tool::ToolRegistry;
/// use agent_mcp::McpServer;
///
/// let mut registry = ToolRegistry::new();
/// // ... register tools ...
/// let server = McpServer::new(registry);
/// server.serve_stdio().await?;
/// ```
pub struct McpServer {
    /// The tool registry containing all available tools.
    registry: Arc<ToolRegistry>,
    /// Server name for identification.
    name: String,
    /// Server version for identification.
    version: String,
    /// Optional instructions for clients.
    instructions: Option<String>,
}

impl McpServer {
    /// Create a new MCP server wrapping the given tool registry.
    pub fn new(registry: ToolRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
            name: "agent-mcp-server".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            instructions: None,
        }
    }

    /// Set the server name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the server version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set instructions for clients.
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Serve via stdio (stdin/stdout).
    ///
    /// This blocks until the client disconnects.
    ///
    /// # Errors
    ///
    /// Returns [`AgentMcpError`] if the server fails to start.
    pub async fn serve_stdio(self) -> Result<(), AgentMcpError> {
        use rmcp::transport::io::stdio;
        use rmcp::ServiceExt;

        let transport = stdio();
        let service = self
            .serve(transport)
            .await
            .map_err(|e| AgentMcpError::Connection(e.to_string()))?;

        service
            .waiting()
            .await
            .map_err(|e| AgentMcpError::Transport(e.to_string()))?;

        Ok(())
    }

    /// Get a reference to the underlying tool registry.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Convert our ToolDefinition to rmcp's Tool type.
    fn definition_to_rmcp_tool(def: &ToolDefinition) -> RmcpTool {
        let input_schema = match &def.input_schema {
            serde_json::Value::Object(m) => Arc::new(m.clone()),
            _ => Arc::new(serde_json::Map::new()),
        };

        let output_schema = def.output_schema.as_ref().and_then(|s| match s {
            serde_json::Value::Object(m) => Some(Arc::new(m.clone())),
            _ => None,
        });

        let annotations = def.annotations.as_ref().map(|a| RmcpToolAnnotations {
            title: None,
            read_only_hint: a.read_only_hint,
            destructive_hint: a.destructive_hint,
            idempotent_hint: a.idempotent_hint,
            open_world_hint: a.open_world_hint,
        });

        RmcpTool {
            name: Cow::Owned(def.name.clone()),
            title: def.title.clone(),
            description: Some(Cow::Owned(def.description.clone())),
            input_schema,
            output_schema,
            annotations,
            execution: None,
            icons: None,
            meta: None,
        }
    }

    /// Create a default ToolContext for tool execution.
    fn default_tool_context() -> ToolContext {
        ToolContext {
            cwd: std::env::current_dir().unwrap_or_default(),
            session_id: "mcp-server".to_string(),
            environment: HashMap::new(),
            cancellation_token: tokio_util::sync::CancellationToken::new(),
            progress_reporter: None,
        }
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: self.name.clone(),
                title: None,
                version: self.version.clone(),
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: self.instructions.clone(),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        async {
            let definitions = self.registry.definitions();
            let tools: Vec<RmcpTool> = definitions
                .iter()
                .map(Self::definition_to_rmcp_tool)
                .collect();

            Ok(ListToolsResult::with_all_items(tools))
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, ErrorData>> + Send + '_ {
        async move {
            let name = request.name.as_ref();
            let input = match request.arguments {
                Some(args) => serde_json::Value::Object(args),
                None => serde_json::Value::Object(serde_json::Map::new()),
            };

            let ctx = Self::default_tool_context();

            let result = self
                .registry
                .execute(name, input, &ctx)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

            // Convert our ToolOutput to rmcp's CallToolResult
            let content: Vec<Content> = result
                .content
                .into_iter()
                .map(|item| match item {
                    agent_types::ContentItem::Text(text) => Content::text(text),
                    agent_types::ContentItem::Image { source } => match source {
                        agent_types::ImageSource::Base64 {
                            media_type, data, ..
                        } => Content::image(data, media_type),
                        agent_types::ImageSource::Url { url } => {
                            // rmcp doesn't have a URL image type; send as text
                            Content::text(format!("[image: {}]", url))
                        }
                    },
                })
                .collect();

            Ok(CallToolResult {
                content,
                structured_content: result.structured_content,
                is_error: if result.is_error { Some(true) } else { None },
                meta: None,
            })
        }
    }

    fn get_tool(&self, name: &str) -> Option<RmcpTool> {
        self.registry
            .get(name)
            .map(|t| Self::definition_to_rmcp_tool(&t.definition()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_creation() {
        let registry = ToolRegistry::new();
        let server = McpServer::new(registry);
        assert_eq!(server.name, "agent-mcp-server");
    }

    #[test]
    fn server_with_name() {
        let registry = ToolRegistry::new();
        let server = McpServer::new(registry)
            .with_name("my-server")
            .with_version("1.0.0")
            .with_instructions("Use this server for testing");

        assert_eq!(server.name, "my-server");
        assert_eq!(server.version, "1.0.0");
        assert_eq!(server.instructions, Some("Use this server for testing".to_string()));
    }

    #[test]
    fn server_get_info() {
        let registry = ToolRegistry::new();
        let server = McpServer::new(registry).with_name("test");
        let info = server.get_info();

        assert_eq!(info.server_info.name, "test");
        assert!(info.capabilities.tools.is_some());
    }

    #[test]
    fn definition_to_rmcp_tool_conversion() {
        let def = ToolDefinition {
            name: "greet".to_string(),
            title: Some("Greeter".to_string()),
            description: "Greets someone".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                }
            }),
            output_schema: None,
            annotations: Some(agent_types::ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: None,
                idempotent_hint: None,
                open_world_hint: None,
            }),
            cache_control: None,
        };

        let rmcp_tool = McpServer::definition_to_rmcp_tool(&def);
        assert_eq!(rmcp_tool.name.as_ref(), "greet");
        assert_eq!(rmcp_tool.title, Some("Greeter".to_string()));
        assert_eq!(rmcp_tool.description.as_deref(), Some("Greets someone"));
        assert!(rmcp_tool.annotations.is_some());
        assert_eq!(rmcp_tool.annotations.as_ref().and_then(|a| a.read_only_hint), Some(true));
    }

    #[test]
    fn get_tool_returns_none_for_unknown() {
        let registry = ToolRegistry::new();
        let server = McpServer::new(registry);
        assert!(server.get_tool("nonexistent").is_none());
    }
}
