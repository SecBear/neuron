//! Concrete [`ObservabilityHook`] using the [`tracing`] crate.
//!
//! Emits structured `tracing` events for each stage of the agentic loop.
//! Wire to any `tracing`-compatible subscriber (`tracing-subscriber` for
//! stdout, `tracing-opentelemetry` for OpenTelemetry export).

use neuron_types::{HookAction, HookError, HookEvent, ObservabilityHook};

/// An [`ObservabilityHook`] that emits structured [`tracing`] events.
///
/// Always returns [`HookAction::Continue`] — observes but never controls.
///
/// # Span levels
///
/// | Event | Level |
/// |-------|-------|
/// | LoopIteration, PreLlmCall, PostLlmCall, PreToolExecution, PostToolExecution | `DEBUG` |
/// | ContextCompaction, SessionStart, SessionEnd | `INFO` |
///
/// # Example
///
/// ```no_run
/// use neuron_runtime::TracingHook;
///
/// let hook = TracingHook::new();
/// // Pass to AgentLoop::builder(...).hook(hook).build()
/// ```
pub struct TracingHook;

impl TracingHook {
    /// Create a new `TracingHook`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for TracingHook {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservabilityHook for TracingHook {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl std::future::Future<Output = Result<HookAction, HookError>> + Send {
        match &event {
            HookEvent::LoopIteration { turn } => {
                tracing::debug!(turn, "neuron.loop.iteration");
            }
            HookEvent::PreLlmCall { request } => {
                tracing::debug!(
                    model = %request.model,
                    messages = request.messages.len(),
                    tools = request.tools.len(),
                    "neuron.llm.pre_call"
                );
            }
            HookEvent::PostLlmCall { response } => {
                tracing::debug!(
                    model = %response.model,
                    stop_reason = ?response.stop_reason,
                    input_tokens = response.usage.input_tokens,
                    output_tokens = response.usage.output_tokens,
                    "neuron.llm.post_call"
                );
            }
            HookEvent::PreToolExecution { tool_name, .. } => {
                tracing::debug!(tool = %tool_name, "neuron.tool.pre_execution");
            }
            HookEvent::PostToolExecution { tool_name, output } => {
                tracing::debug!(
                    tool = %tool_name,
                    is_error = output.is_error,
                    "neuron.tool.post_execution"
                );
            }
            HookEvent::ContextCompaction {
                old_tokens,
                new_tokens,
            } => {
                tracing::info!(
                    old_tokens,
                    new_tokens,
                    reduced_by = old_tokens - new_tokens,
                    "neuron.context.compaction"
                );
            }
            HookEvent::SessionStart { session_id } => {
                tracing::info!(session_id, "neuron.session.start");
            }
            HookEvent::SessionEnd { session_id } => {
                tracing::info!(session_id, "neuron.session.end");
            }
        }
        std::future::ready(Ok(HookAction::Continue))
    }
}
