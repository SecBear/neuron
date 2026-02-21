//! Built-in middleware implementations.

use std::sync::Arc;

use agent_types::{
    ContentItem, PermissionDecision, PermissionPolicy, ToolContext, ToolError, ToolOutput,
    WasmBoxedFuture,
};

use crate::middleware::{Next, ToolCall, ToolMiddleware};

/// Middleware that checks tool call permissions against a [`PermissionPolicy`].
///
/// If the policy returns `Deny`, the tool call is rejected with `ToolError::PermissionDenied`.
/// If the policy returns `Ask`, the tool call is rejected (external confirmation not handled here).
pub struct PermissionChecker {
    policy: Arc<dyn PermissionPolicy>,
}

impl PermissionChecker {
    /// Create a new permission checker with the given policy.
    pub fn new(policy: impl PermissionPolicy + 'static) -> Self {
        Self {
            policy: Arc::new(policy),
        }
    }
}

impl ToolMiddleware for PermissionChecker {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            match self.policy.check(&call.name, &call.input) {
                PermissionDecision::Allow => next.run(call, ctx).await,
                PermissionDecision::Deny(reason) => {
                    Err(ToolError::PermissionDenied(reason))
                }
                PermissionDecision::Ask(reason) => {
                    Err(ToolError::PermissionDenied(format!(
                        "requires confirmation: {reason}"
                    )))
                }
            }
        })
    }
}

/// Middleware that truncates tool output to a maximum character length.
///
/// Long tool outputs can consume excessive tokens in the context window.
/// This middleware truncates text content items that exceed the limit.
pub struct OutputFormatter {
    max_chars: usize,
}

impl OutputFormatter {
    /// Create a new output formatter with the given character limit.
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }
}

impl ToolMiddleware for OutputFormatter {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let mut output = next.run(call, ctx).await?;

            // Truncate text content items that exceed the limit
            output.content = output
                .content
                .into_iter()
                .map(|item| match item {
                    ContentItem::Text(text) if text.len() > self.max_chars => {
                        ContentItem::Text(format!(
                            "{}... [truncated, {} chars total]",
                            &text[..self.max_chars],
                            text.len()
                        ))
                    }
                    other => other,
                })
                .collect();

            Ok(output)
        })
    }
}
