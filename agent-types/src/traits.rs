//! Core traits: Provider, Tool, ToolDyn, ContextStrategy, ObservabilityHook, DurableContext.

use std::future::Future;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{ProviderError, ToolError};
use crate::stream::StreamHandle;
use crate::types::{
    CompletionRequest, CompletionResponse, ContentItem, ToolContext, ToolDefinition, ToolOutput,
};
use crate::wasm::{WasmBoxedFuture, WasmCompatSend, WasmCompatSync};

/// LLM provider trait. Implement this for each provider (Anthropic, OpenAI, Ollama, etc.).
///
/// Uses RPITIT (return position impl trait in trait) — Rust 2024 native async.
/// Not object-safe by design; use generics `<P: Provider>` to compose.
///
/// # Example
///
/// ```ignore
/// struct MyProvider;
///
/// impl Provider for MyProvider {
///     fn complete(&self, request: CompletionRequest)
///         -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send
///     {
///         async { todo!() }
///     }
///
///     fn complete_stream(&self, request: CompletionRequest)
///         -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send
///     {
///         async { todo!() }
///     }
/// }
/// ```
pub trait Provider: WasmCompatSend + WasmCompatSync {
    /// Send a completion request and get a full response.
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + WasmCompatSend;

    /// Send a completion request and get a stream of events.
    fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + WasmCompatSend;
}

/// Strongly-typed tool trait. Implement this for your tools.
///
/// The blanket impl of [`ToolDyn`] handles JSON deserialization/serialization
/// so you work with concrete Rust types.
///
/// # Example
///
/// ```ignore
/// use agent_types::*;
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize, schemars::JsonSchema)]
/// struct MyArgs { query: String }
///
/// struct MyTool;
/// impl Tool for MyTool {
///     const NAME: &'static str = "my_tool";
///     type Args = MyArgs;
///     type Output = String;
///     type Error = std::io::Error;
///
///     fn definition(&self) -> ToolDefinition { todo!() }
///     fn call(&self, args: MyArgs, ctx: &ToolContext)
///         -> impl Future<Output = Result<String, std::io::Error>> + Send
///     { async { Ok(args.query) } }
/// }
/// ```
pub trait Tool: WasmCompatSend + WasmCompatSync {
    /// The unique name of this tool.
    const NAME: &'static str;
    /// The deserialized input type.
    type Args: DeserializeOwned + schemars::JsonSchema + WasmCompatSend;
    /// The serializable output type.
    type Output: Serialize;
    /// The tool-specific error type.
    type Error: std::error::Error + WasmCompatSend + 'static;

    /// Returns the tool definition (name, description, schema).
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with typed arguments.
    fn call(
        &self,
        args: Self::Args,
        ctx: &ToolContext,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + WasmCompatSend;
}

/// Type-erased tool for dynamic dispatch. Blanket-implemented for all [`Tool`] impls.
///
/// This enables heterogeneous tool collections (`HashMap<String, Arc<dyn ToolDyn>>`)
/// while preserving type safety at the implementation level.
pub trait ToolDyn: WasmCompatSend + WasmCompatSync {
    /// The tool's unique name.
    fn name(&self) -> &str;
    /// The tool definition (name, description, input schema).
    fn definition(&self) -> ToolDefinition;
    /// Execute the tool with a JSON value input, returning a generic output.
    fn call_dyn<'a>(
        &'a self,
        input: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>>;
}

/// Blanket implementation: any `Tool` automatically becomes a `ToolDyn`.
///
/// Handles:
/// - Deserializing `serde_json::Value` into `T::Args`
/// - Calling `T::call(args, ctx)`
/// - Serializing `T::Output` into `ToolOutput`
/// - Mapping `T::Error` into `ToolError::ExecutionFailed`
impl<T: Tool> ToolDyn for T {
    fn name(&self) -> &str {
        T::NAME
    }

    fn definition(&self) -> ToolDefinition {
        Tool::definition(self)
    }

    fn call_dyn<'a>(
        &'a self,
        input: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let args: T::Args = serde_json::from_value(input)
                .map_err(|e| ToolError::InvalidInput(e.to_string()))?;

            let output = self.call(args, ctx).await.map_err(|e| {
                ToolError::ExecutionFailed(e.to_string().into())
            })?;

            let structured = serde_json::to_value(&output)
                .map_err(|e| ToolError::ExecutionFailed(Box::new(e)))?;

            let text = match &structured {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };

            Ok(ToolOutput {
                content: vec![ContentItem::Text(text)],
                structured_content: Some(structured),
                is_error: false,
            })
        })
    }
}
