//! Tool execution sandboxing.
//!
//! The [`Sandbox`] trait wraps tool execution with isolation, resource limits,
//! or security boundaries. [`NoOpSandbox`] passes through directly.

use std::future::Future;

use agent_types::{SandboxError, ToolContext, ToolDyn, ToolOutput, WasmCompatSend, WasmCompatSync};

/// Sandbox for isolating tool execution.
///
/// Implementations can wrap tool calls with filesystem isolation,
/// network restrictions, resource limits, or container boundaries.
///
/// # Example
///
/// ```ignore
/// use agent_runtime::*;
/// use agent_types::*;
///
/// struct NoOpSandbox;
/// impl Sandbox for NoOpSandbox {
///     fn execute_tool(
///         &self,
///         tool: &dyn ToolDyn,
///         input: serde_json::Value,
///         ctx: &ToolContext,
///     ) -> impl Future<Output = Result<ToolOutput, SandboxError>> + Send {
///         async move {
///             tool.call_dyn(input, ctx)
///                 .await
///                 .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))
///         }
///     }
/// }
/// ```
pub trait Sandbox: WasmCompatSend + WasmCompatSync {
    /// Execute a tool within the sandbox.
    fn execute_tool(
        &self,
        tool: &dyn ToolDyn,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> impl Future<Output = Result<ToolOutput, SandboxError>> + WasmCompatSend;
}

/// A no-op sandbox that passes tool execution through directly.
///
/// Use this when no sandboxing is needed.
pub struct NoOpSandbox;

impl Sandbox for NoOpSandbox {
    async fn execute_tool(
        &self,
        tool: &dyn ToolDyn,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, SandboxError> {
        tool.call_dyn(input, ctx)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))
    }
}
