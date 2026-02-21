//! Core traits: Provider, Tool, ToolDyn, ContextStrategy, ObservabilityHook, DurableContext.

use std::future::Future;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{ContextError, DurableError, HookError, ProviderError, ToolError};
use crate::stream::StreamHandle;
use crate::types::{
    CompletionRequest, CompletionResponse, ContentItem, Message, ToolContext, ToolDefinition,
    ToolOutput,
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

// --- Context Strategy ---

/// Strategy for compacting conversation context when it exceeds token limits.
///
/// Implementations decide when to compact and how to reduce the message list.
///
/// # Example
///
/// ```ignore
/// struct KeepLastN { n: usize }
/// impl ContextStrategy for KeepLastN {
///     fn should_compact(&self, _messages: &[Message], token_count: usize) -> bool {
///         token_count > 100_000
///     }
///     fn compact(&self, messages: Vec<Message>)
///         -> impl Future<Output = Result<Vec<Message>, ContextError>> + Send
///     {
///         async move { Ok(messages.into_iter().rev().take(self.n).collect()) }
///     }
///     fn token_estimate(&self, messages: &[Message]) -> usize { messages.len() * 100 }
/// }
/// ```
pub trait ContextStrategy: WasmCompatSend + WasmCompatSync {
    /// Whether compaction should be triggered given the current messages and token count.
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool;

    /// Compact the message list to reduce token usage.
    fn compact(
        &self,
        messages: Vec<Message>,
    ) -> impl Future<Output = Result<Vec<Message>, ContextError>> + WasmCompatSend;

    /// Estimate the token count for a list of messages.
    fn token_estimate(&self, messages: &[Message]) -> usize;
}

// --- Observability Hooks ---

/// Events fired during the agentic loop for observability.
#[derive(Debug)]
pub enum HookEvent<'a> {
    /// Start of a loop iteration.
    LoopIteration {
        /// The current turn number (0-indexed).
        turn: usize,
    },
    /// Before calling the LLM provider.
    PreLlmCall {
        /// The request about to be sent.
        request: &'a CompletionRequest,
    },
    /// After receiving the LLM response.
    PostLlmCall {
        /// The response received.
        response: &'a CompletionResponse,
    },
    /// Before executing a tool.
    PreToolExecution {
        /// Name of the tool.
        tool_name: &'a str,
        /// Input arguments.
        input: &'a serde_json::Value,
    },
    /// After executing a tool.
    PostToolExecution {
        /// Name of the tool.
        tool_name: &'a str,
        /// The tool's output.
        output: &'a ToolOutput,
    },
    /// Context was compacted.
    ContextCompaction {
        /// Token count before compaction.
        old_tokens: usize,
        /// Token count after compaction.
        new_tokens: usize,
    },
    /// A session started.
    SessionStart {
        /// The session identifier.
        session_id: &'a str,
    },
    /// A session ended.
    SessionEnd {
        /// The session identifier.
        session_id: &'a str,
    },
}

/// Action to take after processing a hook event.
#[derive(Debug)]
pub enum HookAction {
    /// Continue normal execution.
    Continue,
    /// Skip the current operation and return a rejection message.
    Skip {
        /// Reason for skipping.
        reason: String,
    },
    /// Terminate the loop immediately.
    Terminate {
        /// Reason for termination.
        reason: String,
    },
}

/// Hook for observability (logging, metrics, telemetry).
///
/// Does NOT control execution flow beyond Continue/Skip/Terminate.
/// For durable execution wrapping, use [`DurableContext`] instead.
///
/// # Example
///
/// ```ignore
/// struct LogHook;
/// impl ObservabilityHook for LogHook {
///     fn on_event(&self, event: HookEvent<'_>)
///         -> impl Future<Output = Result<HookAction, HookError>> + Send
///     {
///         async { println!("{event:?}"); Ok(HookAction::Continue) }
///     }
/// }
/// ```
pub trait ObservabilityHook: WasmCompatSend + WasmCompatSync {
    /// Called for each event in the agentic loop.
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + WasmCompatSend;
}

// --- Durable Context ---

/// Options for executing an activity in a durable context.
#[derive(Debug, Clone)]
pub struct ActivityOptions {
    /// Maximum time for the activity to complete.
    pub start_to_close_timeout: Duration,
    /// Heartbeat interval for long-running activities.
    pub heartbeat_timeout: Option<Duration>,
    /// Retry policy for failed activities.
    pub retry_policy: Option<RetryPolicy>,
}

/// Retry policy for durable activities.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Initial delay before first retry.
    pub initial_interval: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_coefficient: f64,
    /// Maximum number of retry attempts.
    pub maximum_attempts: u32,
    /// Maximum delay between retries.
    pub maximum_interval: Duration,
    /// Error types that should not be retried.
    pub non_retryable_errors: Vec<String>,
}

/// Wraps side effects for durable execution engines (Temporal, Restate, Inngest).
///
/// When present, the agentic loop calls through this instead of directly
/// calling provider/tools, enabling journaling, replay, and crash recovery.
pub trait DurableContext: WasmCompatSend + WasmCompatSync {
    /// Execute an LLM call as a durable activity.
    fn execute_llm_call(
        &self,
        request: CompletionRequest,
        options: ActivityOptions,
    ) -> impl Future<Output = Result<CompletionResponse, DurableError>> + WasmCompatSend;

    /// Execute a tool call as a durable activity.
    fn execute_tool(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
        options: ActivityOptions,
    ) -> impl Future<Output = Result<ToolOutput, DurableError>> + WasmCompatSend;

    /// Wait for an external signal with a timeout.
    fn wait_for_signal<T: DeserializeOwned + WasmCompatSend>(
        &self,
        signal_name: &str,
        timeout: Duration,
    ) -> impl Future<Output = Result<Option<T>, DurableError>> + WasmCompatSend;

    /// Whether the workflow should continue-as-new to avoid history bloat.
    fn should_continue_as_new(&self) -> bool;

    /// Continue the workflow as a new execution with the given state.
    fn continue_as_new(
        &self,
        state: serde_json::Value,
    ) -> impl Future<Output = Result<(), DurableError>> + WasmCompatSend;

    /// Sleep for a duration (durable — survives replay).
    fn sleep(
        &self,
        duration: Duration,
    ) -> impl Future<Output = ()> + WasmCompatSend;

    /// Current time (deterministic during replay).
    fn now(&self) -> chrono::DateTime<chrono::Utc>;
}

// --- Permission Policy ---

/// Decision from a permission check.
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    /// Allow the tool call.
    Allow,
    /// Deny the tool call with a reason.
    Deny(String),
    /// Ask the user for confirmation.
    Ask(String),
}

/// Policy for checking tool call permissions.
///
/// # Example
///
/// ```ignore
/// struct AllowAll;
/// impl PermissionPolicy for AllowAll {
///     fn check(&self, _tool_name: &str, _input: &serde_json::Value) -> PermissionDecision {
///         PermissionDecision::Allow
///     }
/// }
/// ```
pub trait PermissionPolicy: WasmCompatSend + WasmCompatSync {
    /// Check whether a tool call is permitted.
    fn check(&self, tool_name: &str, input: &serde_json::Value) -> PermissionDecision;
}
