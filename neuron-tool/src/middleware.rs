//! Middleware types for the tool execution pipeline.
//!
//! Middleware wraps tool execution with cross-cutting concerns like
//! validation, permissions, logging, and output formatting.
//!
//! The pattern is identical to axum's `from_fn` — each middleware
//! receives a `Next` that it can call to continue the chain, or
//! skip to short-circuit.

use std::sync::Arc;

use neuron_types::{
    ToolContext, ToolDyn, ToolError, ToolOutput, WasmBoxedFuture, WasmCompatSend, WasmCompatSync,
};

/// A tool call in flight through the middleware pipeline.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Unique identifier for this tool call (from the model).
    pub id: String,
    /// Name of the tool being called.
    pub name: String,
    /// JSON input arguments.
    pub input: serde_json::Value,
}

/// Middleware that wraps tool execution.
///
/// Each middleware receives the call, context, and a [`Next`] to continue the chain.
/// Middleware can:
/// - Inspect/modify the call before passing it on
/// - Short-circuit by returning without calling `next.run()`
/// - Inspect/modify the result after the tool executes
///
/// Uses boxed futures for dyn-compatibility (heterogeneous middleware collections).
pub trait ToolMiddleware: WasmCompatSend + WasmCompatSync {
    /// Process a tool call, optionally delegating to the next middleware/tool.
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>>;
}

/// The remaining middleware chain plus the underlying tool.
///
/// Consumed on call to prevent double-invoke.
pub struct Next<'a> {
    tool: &'a dyn ToolDyn,
    middleware: &'a [Arc<dyn ToolMiddleware>],
}

impl<'a> Next<'a> {
    /// Create a new Next from a tool and middleware slice.
    pub(crate) fn new(tool: &'a dyn ToolDyn, middleware: &'a [Arc<dyn ToolMiddleware>]) -> Self {
        Self { tool, middleware }
    }

    /// Continue the middleware chain, eventually calling the tool.
    pub async fn run(self, call: &'a ToolCall, ctx: &'a ToolContext) -> Result<ToolOutput, ToolError> {
        if let Some((head, tail)) = self.middleware.split_first() {
            let next = Next::new(self.tool, tail);
            head.process(call, ctx, next).await
        } else {
            // End of chain — call the actual tool
            self.tool.call_dyn(call.input.clone(), ctx).await
        }
    }
}

/// Wrapper that implements `ToolMiddleware` for a closure returning a boxed future.
struct MiddlewareFn<F> {
    f: F,
}

impl<F> ToolMiddleware for MiddlewareFn<F>
where
    F: for<'a> Fn(
            &'a ToolCall,
            &'a ToolContext,
            Next<'a>,
        ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>>
        + Send
        + Sync,
{
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        (self.f)(call, ctx, next)
    }
}

/// Create middleware from a closure (like axum's `from_fn`).
///
/// The closure must return a `Box::pin(async move { ... })` future.
///
/// # Example
///
/// ```ignore
/// use neuron_tool::*;
///
/// let logging = tool_middleware_fn(|call, ctx, next| {
///     Box::pin(async move {
///         println!("calling {}", call.name);
///         let result = next.run(call, ctx).await;
///         println!("done");
///         result
///     })
/// });
/// ```
#[must_use]
pub fn tool_middleware_fn<F>(f: F) -> impl ToolMiddleware
where
    F: for<'a> Fn(
            &'a ToolCall,
            &'a ToolContext,
            Next<'a>,
        ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>>
        + Send
        + Sync,
{
    MiddlewareFn { f }
}
