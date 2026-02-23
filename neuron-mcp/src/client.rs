//! MCP client: connect to MCP servers via stdio or HTTP.
//!
//! [`McpClient`] wraps an rmcp client peer and provides ergonomic methods
//! for interacting with MCP servers.

use std::borrow::Cow;

use rmcp::ServiceExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
    PaginatedRequestParams, ReadResourceRequestParams,
};
use rmcp::service::{Peer, RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use tokio::process::Command;

use neuron_types::{McpError, ToolDefinition};

use crate::error::{from_client_init_error, from_service_error};
use crate::types::{McpPrompt, McpPromptArgument, McpResource, McpResourceContents, PaginatedList};

/// MCP client that connects to an MCP server and provides tool, resource,
/// and prompt operations.
///
/// Wraps rmcp's client peer for ergonomic lifecycle management.
/// The client keeps the underlying rmcp service alive and provides
/// methods to interact with the MCP server.
pub struct McpClient {
    /// Handle to the running rmcp service, kept alive to maintain the connection.
    /// Not read directly but must not be dropped.
    #[allow(dead_code)]
    service: RunningService<RoleClient, ()>,
    /// Cloned peer for making requests. Peer is internally Arc-based, so Clone is cheap.
    peer: Peer<RoleClient>,
}

impl McpClient {
    /// Connect to an MCP server via a child process (stdio transport).
    ///
    /// Spawns the specified command and communicates over stdin/stdout
    /// using the MCP protocol.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying the command, args, and environment.
    ///
    /// # Errors
    ///
    /// Returns [`McpError::Connection`] if the process cannot be spawned,
    /// or [`McpError::Initialization`] if the MCP handshake fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let client = McpClient::connect_stdio(StdioConfig {
    ///     command: "npx".to_string(),
    ///     args: vec!["-y".to_string(), "@modelcontextprotocol/server-everything".to_string()],
    ///     env: vec![],
    /// }).await?;
    /// ```
    pub async fn connect_stdio(config: StdioConfig) -> Result<Self, McpError> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let transport =
            TokioChildProcess::new(cmd).map_err(|e| McpError::Connection(e.to_string()))?;

        let service = ().serve(transport).await.map_err(from_client_init_error)?;

        let peer = service.peer().clone();

        Ok(Self { service, peer })
    }

    /// Connect to an MCP server via Streamable HTTP transport.
    ///
    /// Connects to the specified URL using HTTP with Server-Sent Events
    /// for streaming responses.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying the URL, auth, and headers.
    ///
    /// # Errors
    ///
    /// Returns [`McpError::Connection`] if the HTTP connection fails,
    /// or [`McpError::Initialization`] if the MCP handshake fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let client = McpClient::connect_http(HttpConfig {
    ///     url: "http://localhost:8080/mcp".to_string(),
    ///     auth_header: Some("Bearer my-token".to_string()),
    ///     headers: vec![],
    /// }).await?;
    /// ```
    pub async fn connect_http(config: HttpConfig) -> Result<Self, McpError> {
        use rmcp::transport::StreamableHttpClientTransport;

        let transport = if let Some(auth) = config.auth_header {
            use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;

            let transport_config =
                StreamableHttpClientTransportConfig::with_uri(&*config.url).auth_header(auth);

            StreamableHttpClientTransport::from_config(transport_config)
        } else {
            StreamableHttpClientTransport::from_uri(&*config.url)
        };

        let service = ().serve(transport).await.map_err(from_client_init_error)?;

        let peer = service.peer().clone();

        Ok(Self { service, peer })
    }

    /// Get a reference to the underlying rmcp peer for advanced operations.
    #[must_use]
    pub fn peer(&self) -> &Peer<RoleClient> {
        &self.peer
    }

    /// Check whether the transport connection has been closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.peer.is_transport_closed()
    }

    /// List all tools available on the MCP server.
    ///
    /// Returns a paginated list. Use `cursor` from the result to fetch
    /// subsequent pages.
    ///
    /// # Errors
    ///
    /// Returns [`McpError::ToolCall`] if the server returns an error.
    pub async fn list_tools(
        &self,
        cursor: Option<String>,
    ) -> Result<PaginatedList<ToolDefinition>, McpError> {
        let params = cursor.map(|c| PaginatedRequestParams {
            cursor: Some(c),
            meta: None,
        });

        let result = self
            .peer
            .list_tools(params)
            .await
            .map_err(from_service_error)?;

        let tools = result
            .tools
            .into_iter()
            .map(mcp_tool_to_definition)
            .collect();

        Ok(PaginatedList {
            items: tools,
            next_cursor: result.next_cursor.map(|c| c.to_string()),
        })
    }

    /// List all tools without pagination (fetches all pages).
    ///
    /// # Errors
    ///
    /// Returns [`McpError::ToolCall`] if the server returns an error.
    pub async fn list_all_tools(&self) -> Result<Vec<ToolDefinition>, McpError> {
        let tools = self
            .peer
            .list_all_tools()
            .await
            .map_err(from_service_error)?;

        Ok(tools.into_iter().map(mcp_tool_to_definition).collect())
    }

    /// Call a tool on the MCP server.
    ///
    /// # Arguments
    ///
    /// * `name` - The tool name.
    /// * `arguments` - JSON arguments to pass to the tool.
    ///
    /// # Errors
    ///
    /// Returns [`McpError::ToolCall`] if the server returns an error.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        let params = CallToolRequestParams {
            name: Cow::Owned(name.to_string()),
            arguments: arguments.map(|m| m.into_iter().collect()),
            meta: None,
            task: None,
        };

        self.peer
            .call_tool(params)
            .await
            .map_err(from_service_error)
    }

    /// Call a tool with a JSON value as arguments.
    ///
    /// Convenience wrapper that accepts any `serde_json::Value`.
    /// If the value is an Object, its fields become the tool arguments.
    /// If it is Null, no arguments are sent.
    ///
    /// # Errors
    ///
    /// Returns [`McpError::ToolCall`] if the value is not an object or null,
    /// or if the server returns an error.
    pub async fn call_tool_json(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let map = match arguments {
            serde_json::Value::Object(m) => Some(m),
            serde_json::Value::Null => None,
            other => {
                return Err(McpError::ToolCall(format!(
                    "expected object or null arguments, got {other}",
                )));
            }
        };
        self.call_tool(name, map).await
    }

    /// List resources available on the MCP server.
    ///
    /// # Errors
    ///
    /// Returns [`McpError`] if the server returns an error.
    pub async fn list_resources(
        &self,
        cursor: Option<String>,
    ) -> Result<PaginatedList<McpResource>, McpError> {
        let params = cursor.map(|c| PaginatedRequestParams {
            cursor: Some(c),
            meta: None,
        });

        let result = self
            .peer
            .list_resources(params)
            .await
            .map_err(from_service_error)?;

        let resources = result
            .resources
            .into_iter()
            .map(|r| {
                let raw = r.raw;
                McpResource {
                    uri: raw.uri,
                    name: raw.name,
                    title: raw.title,
                    description: raw.description,
                    mime_type: raw.mime_type,
                }
            })
            .collect();

        Ok(PaginatedList {
            items: resources,
            next_cursor: result.next_cursor.map(|c| c.to_string()),
        })
    }

    /// Read a resource from the MCP server.
    ///
    /// # Errors
    ///
    /// Returns [`McpError`] if the server returns an error.
    pub async fn read_resource(&self, uri: &str) -> Result<Vec<McpResourceContents>, McpError> {
        let params = ReadResourceRequestParams {
            uri: uri.to_string(),
            meta: None,
        };

        let result = self
            .peer
            .read_resource(params)
            .await
            .map_err(from_service_error)?;

        let contents = result
            .contents
            .into_iter()
            .map(|c| match c {
                rmcp::model::ResourceContents::TextResourceContents {
                    uri,
                    mime_type,
                    text,
                    ..
                } => McpResourceContents {
                    uri,
                    mime_type,
                    text: Some(text),
                    blob: None,
                },
                rmcp::model::ResourceContents::BlobResourceContents {
                    uri,
                    mime_type,
                    blob,
                    ..
                } => McpResourceContents {
                    uri,
                    mime_type,
                    text: None,
                    blob: Some(blob),
                },
            })
            .collect();

        Ok(contents)
    }

    /// List prompts available on the MCP server.
    ///
    /// # Errors
    ///
    /// Returns [`McpError`] if the server returns an error.
    pub async fn list_prompts(
        &self,
        cursor: Option<String>,
    ) -> Result<PaginatedList<McpPrompt>, McpError> {
        let params = cursor.map(|c| PaginatedRequestParams {
            cursor: Some(c),
            meta: None,
        });

        let result = self
            .peer
            .list_prompts(params)
            .await
            .map_err(from_service_error)?;

        let prompts = result
            .prompts
            .into_iter()
            .map(|p| McpPrompt {
                name: p.name,
                title: p.title,
                description: p.description,
                arguments: p
                    .arguments
                    .unwrap_or_default()
                    .into_iter()
                    .map(|a| McpPromptArgument {
                        name: a.name,
                        description: a.description,
                        required: a.required,
                    })
                    .collect(),
            })
            .collect();

        Ok(PaginatedList {
            items: prompts,
            next_cursor: result.next_cursor.map(|c| c.to_string()),
        })
    }

    /// Get a prompt from the MCP server.
    ///
    /// # Errors
    ///
    /// Returns [`McpError`] if the server returns an error.
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<GetPromptResult, McpError> {
        let params = GetPromptRequestParams {
            name: name.to_string(),
            arguments: arguments.map(|m| m.into_iter().collect()),
            meta: None,
        };

        self.peer
            .get_prompt(params)
            .await
            .map_err(from_service_error)
    }
}

/// Configuration for an MCP stdio connection.
pub struct StdioConfig {
    /// The command to spawn.
    pub command: String,
    /// Arguments to pass to the command.
    pub args: Vec<String>,
    /// Environment variables to set.
    pub env: Vec<(String, String)>,
}

/// Configuration for an MCP HTTP connection.
pub struct HttpConfig {
    /// The server URL.
    pub url: String,
    /// Optional authorization header value.
    pub auth_header: Option<String>,
    /// Additional HTTP headers.
    pub headers: Vec<(String, String)>,
}

/// Convert an rmcp `Tool` to our `ToolDefinition`.
pub(crate) fn mcp_tool_to_definition(tool: rmcp::model::Tool) -> ToolDefinition {
    // Convert the rmcp JsonObject (Arc<Map<String, Value>>) to serde_json::Value
    let input_schema = serde_json::Value::Object(tool.input_schema.as_ref().clone());

    let output_schema = tool
        .output_schema
        .map(|s| serde_json::Value::Object(s.as_ref().clone()));

    let annotations = tool.annotations.map(|a| neuron_types::ToolAnnotations {
        read_only_hint: a.read_only_hint,
        destructive_hint: a.destructive_hint,
        idempotent_hint: a.idempotent_hint,
        open_world_hint: a.open_world_hint,
    });

    ToolDefinition {
        name: tool.name.into_owned(),
        title: tool.title,
        description: tool.description.map(|d| d.into_owned()).unwrap_or_default(),
        input_schema,
        output_schema,
        annotations,
        cache_control: None,
    }
}

/// Convert an rmcp `CallToolResult` to our `ToolOutput`.
pub(crate) fn call_tool_result_to_output(result: CallToolResult) -> neuron_types::ToolOutput {
    let content = result
        .content
        .into_iter()
        .filter_map(|c| {
            // Annotated<RawContent> -- access raw via deref
            match &*c {
                rmcp::model::RawContent::Text(t) => {
                    Some(neuron_types::ContentItem::Text(t.text.clone()))
                }
                rmcp::model::RawContent::Image(img) => Some(neuron_types::ContentItem::Image {
                    source: neuron_types::ImageSource::Base64 {
                        media_type: img.mime_type.clone(),
                        data: img.data.clone(),
                    },
                }),
                // Audio, Resource, ResourceLink content types don't map to our ContentItem
                _ => None,
            }
        })
        .collect();

    neuron_types::ToolOutput {
        content,
        structured_content: result.structured_content,
        is_error: result.is_error.unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_config_fields() {
        let config = StdioConfig {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            env: vec![("FOO".to_string(), "bar".to_string())],
        };
        assert_eq!(config.command, "echo");
        assert_eq!(config.args.len(), 1);
        assert_eq!(config.env.len(), 1);
    }

    #[test]
    fn http_config_fields() {
        let config = HttpConfig {
            url: "http://localhost:8080".to_string(),
            auth_header: Some("Bearer token".to_string()),
            headers: vec![],
        };
        assert_eq!(config.url, "http://localhost:8080");
        assert!(config.auth_header.is_some());
    }

    #[test]
    fn mcp_tool_conversion() {
        use std::sync::Arc;
        let mut schema = serde_json::Map::new();
        schema.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );

        let tool = rmcp::model::Tool {
            name: Cow::Borrowed("test_tool"),
            title: Some("Test Tool".to_string()),
            description: Some(Cow::Borrowed("A test tool")),
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        };

        let def = mcp_tool_to_definition(tool);
        assert_eq!(def.name, "test_tool");
        assert_eq!(def.title, Some("Test Tool".to_string()));
        assert_eq!(def.description, "A test tool");
    }

    #[test]
    fn mcp_tool_conversion_with_annotations() {
        use std::sync::Arc;
        let schema = serde_json::Map::new();

        let annotations = rmcp::model::ToolAnnotations {
            title: None,
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        };

        let tool = rmcp::model::Tool {
            name: Cow::Borrowed("annotated_tool"),
            title: None,
            description: Some(Cow::Borrowed("A tool with annotations")),
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: Some(annotations),
            execution: None,
            icons: None,
            meta: None,
        };

        let def = mcp_tool_to_definition(tool);
        assert_eq!(def.name, "annotated_tool");
        let ann = def.annotations.expect("should have annotations");
        assert_eq!(ann.read_only_hint, Some(true));
        assert_eq!(ann.destructive_hint, Some(false));
        assert_eq!(ann.idempotent_hint, Some(true));
        assert_eq!(ann.open_world_hint, Some(false));
    }

    #[test]
    fn mcp_tool_conversion_no_description() {
        use std::sync::Arc;
        let schema = serde_json::Map::new();

        let tool = rmcp::model::Tool {
            name: Cow::Borrowed("no_desc"),
            title: None,
            description: None,
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        };

        let def = mcp_tool_to_definition(tool);
        assert_eq!(def.name, "no_desc");
        assert_eq!(def.description, "");
    }

    #[test]
    fn call_tool_result_to_output_text() {
        use rmcp::model::Content;

        let result = CallToolResult {
            content: vec![Content::text("hello world")],
            structured_content: None,
            is_error: None,
            meta: None,
        };

        let output = call_tool_result_to_output(result);
        assert!(!output.is_error);
        assert_eq!(output.content.len(), 1);
        match &output.content[0] {
            neuron_types::ContentItem::Text(t) => assert_eq!(t, "hello world"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn call_tool_result_to_output_error() {
        let result = CallToolResult {
            content: vec![],
            structured_content: None,
            is_error: Some(true),
            meta: None,
        };

        let output = call_tool_result_to_output(result);
        assert!(output.is_error);
    }

    #[test]
    fn call_tool_result_to_output_image() {
        use rmcp::model::Content;

        let result = CallToolResult {
            content: vec![Content::image("aW1hZ2VkYXRh", "image/png")],
            structured_content: None,
            is_error: None,
            meta: None,
        };

        let output = call_tool_result_to_output(result);
        assert!(!output.is_error);
        assert_eq!(output.content.len(), 1);
        match &output.content[0] {
            neuron_types::ContentItem::Image { source } => match source {
                neuron_types::ImageSource::Base64 {
                    media_type, data, ..
                } => {
                    assert_eq!(media_type, "image/png");
                    assert_eq!(data, "aW1hZ2VkYXRh");
                }
                _ => panic!("expected base64 image source"),
            },
            _ => panic!("expected image content"),
        }
    }

    #[test]
    fn call_tool_result_to_output_multiple_content() {
        use rmcp::model::Content;

        let result = CallToolResult {
            content: vec![
                Content::text("first"),
                Content::text("second"),
                Content::image("data", "image/jpeg"),
            ],
            structured_content: None,
            is_error: None,
            meta: None,
        };

        let output = call_tool_result_to_output(result);
        assert_eq!(output.content.len(), 3);
        match &output.content[0] {
            neuron_types::ContentItem::Text(t) => assert_eq!(t, "first"),
            _ => panic!("expected text"),
        }
        match &output.content[1] {
            neuron_types::ContentItem::Text(t) => assert_eq!(t, "second"),
            _ => panic!("expected text"),
        }
        match &output.content[2] {
            neuron_types::ContentItem::Image { .. } => {}
            _ => panic!("expected image"),
        }
    }

    #[test]
    fn call_tool_result_to_output_filters_unsupported_content() {
        use rmcp::model::{AnnotateAble, Content, RawAudioContent, RawContent};

        // Audio content should be filtered out
        let audio_raw = RawContent::Audio(RawAudioContent {
            data: "audiodata".to_string(),
            mime_type: "audio/wav".to_string(),
        });
        let audio_content: Content = audio_raw.no_annotation();

        let result = CallToolResult {
            content: vec![Content::text("kept"), audio_content],
            structured_content: None,
            is_error: None,
            meta: None,
        };

        let output = call_tool_result_to_output(result);
        // Only text should survive; audio is filtered
        assert_eq!(output.content.len(), 1);
        match &output.content[0] {
            neuron_types::ContentItem::Text(t) => assert_eq!(t, "kept"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn call_tool_result_to_output_structured_content() {
        let structured = serde_json::json!({"result": 42});

        let result = CallToolResult {
            content: vec![],
            structured_content: Some(structured.clone()),
            is_error: None,
            meta: None,
        };

        let output = call_tool_result_to_output(result);
        assert_eq!(output.structured_content, Some(structured));
        assert!(!output.is_error);
    }

    #[test]
    fn call_tool_result_to_output_is_error_false() {
        let result = CallToolResult {
            content: vec![],
            structured_content: None,
            is_error: Some(false),
            meta: None,
        };

        let output = call_tool_result_to_output(result);
        assert!(!output.is_error);
    }

    #[test]
    fn call_tool_result_to_output_empty() {
        let result = CallToolResult {
            content: vec![],
            structured_content: None,
            is_error: None,
            meta: None,
        };

        let output = call_tool_result_to_output(result);
        assert!(output.content.is_empty());
        assert!(output.structured_content.is_none());
        assert!(!output.is_error);
    }

    #[test]
    fn mcp_tool_conversion_with_output_schema() {
        use std::sync::Arc;
        let input_schema = serde_json::Map::new();
        let mut output_schema = serde_json::Map::new();
        output_schema.insert(
            "type".to_string(),
            serde_json::Value::String("string".to_string()),
        );

        let tool = rmcp::model::Tool {
            name: Cow::Borrowed("with_output"),
            title: None,
            description: Some(Cow::Borrowed("Has output schema")),
            input_schema: Arc::new(input_schema),
            output_schema: Some(Arc::new(output_schema.clone())),
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        };

        let def = mcp_tool_to_definition(tool);
        assert_eq!(def.name, "with_output");
        let os = def.output_schema.expect("should have output_schema");
        assert_eq!(os, serde_json::Value::Object(output_schema));
    }

    #[test]
    fn mcp_tool_conversion_owned_name() {
        use std::sync::Arc;
        let schema = serde_json::Map::new();

        let tool = rmcp::model::Tool {
            name: Cow::Owned("owned_name".to_string()),
            title: None,
            description: Some(Cow::Owned("Owned desc".to_string())),
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        };

        let def = mcp_tool_to_definition(tool);
        assert_eq!(def.name, "owned_name");
        assert_eq!(def.description, "Owned desc");
    }

    #[test]
    fn mcp_tool_conversion_preserves_complex_schema() {
        use std::sync::Arc;
        let mut schema = serde_json::Map::new();
        schema.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );
        let mut props = serde_json::Map::new();
        props.insert("a".to_string(), serde_json::json!({"type": "integer"}));
        props.insert(
            "b".to_string(),
            serde_json::json!({"type": "array", "items": {"type": "string"}}),
        );
        schema.insert("properties".to_string(), serde_json::Value::Object(props));
        schema.insert("required".to_string(), serde_json::json!(["a"]));

        let expected = serde_json::Value::Object(schema.clone());

        let tool = rmcp::model::Tool {
            name: Cow::Borrowed("complex"),
            title: None,
            description: None,
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        };

        let def = mcp_tool_to_definition(tool);
        assert_eq!(def.input_schema, expected);
    }

    #[test]
    fn mcp_tool_cache_control_is_none() {
        use std::sync::Arc;
        let schema = serde_json::Map::new();

        let tool = rmcp::model::Tool {
            name: Cow::Borrowed("any"),
            title: None,
            description: None,
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        };

        let def = mcp_tool_to_definition(tool);
        assert!(def.cache_control.is_none());
    }

    #[test]
    fn http_config_no_auth() {
        let config = HttpConfig {
            url: "http://example.com/mcp".to_string(),
            auth_header: None,
            headers: vec![("X-Custom".to_string(), "value".to_string())],
        };
        assert!(config.auth_header.is_none());
        assert_eq!(config.headers.len(), 1);
    }

    #[test]
    fn stdio_config_empty_env() {
        let config = StdioConfig {
            command: "cat".to_string(),
            args: vec![],
            env: vec![],
        };
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
    }
}
