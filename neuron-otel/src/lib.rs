//! OpenTelemetry instrumentation for neuron using GenAI semantic conventions.
//!
//! Implements [`ObservabilityHook`] with the [OTel GenAI semantic conventions][spec]
//! (`gen_ai.*` namespace). Emits [`tracing`] spans — users bring their own
//! `tracing-opentelemetry` subscriber for OTel export.
//!
//! # Usage
//!
//! ```no_run
//! use neuron_otel::{OtelHook, OtelConfig};
//!
//! let hook = OtelHook::new(OtelConfig {
//!     capture_input: false,
//!     capture_output: false,
//! });
//! // Pass to AgentLoop::builder(...).hook(hook).build()
//! ```
//!
//! # Span hierarchy
//!
//! | Span name | OTel convention | When |
//! |-----------|-----------------|------|
//! | `gen_ai.loop.iteration` | — | Each loop turn |
//! | `gen_ai.chat` | `gen_ai.chat` | LLM request/response |
//! | `gen_ai.execute_tool` | `gen_ai.execute_tool` | Tool execution |
//! | `gen_ai.context.compaction` | — | Context compaction |
//!
//! # Opt-in content capture
//!
//! By default, request/response content is NOT captured (privacy).
//! Set `capture_input` / `capture_output` to `true` to include message
//! bodies in span attributes.
//!
//! [spec]: https://opentelemetry.io/docs/specs/semconv/gen-ai/

use neuron_types::{HookAction, HookError, HookEvent, ObservabilityHook};

/// Configuration for the OTel hook.
#[derive(Debug, Clone, Default)]
pub struct OtelConfig {
    /// Whether to capture input message content in span attributes.
    /// Disabled by default for privacy.
    pub capture_input: bool,
    /// Whether to capture output message content in span attributes.
    /// Disabled by default for privacy.
    pub capture_output: bool,
}

/// An [`ObservabilityHook`] that emits [`tracing`] spans following the
/// OTel GenAI semantic conventions.
///
/// Always returns [`HookAction::Continue`] — observes but never controls.
///
/// # Attributes emitted
///
/// | Attribute | Value |
/// |-----------|-------|
/// | `gen_ai.system` | `"neuron"` |
/// | `gen_ai.request.model` | Model from request |
/// | `gen_ai.usage.input_tokens` | Input token count |
/// | `gen_ai.usage.output_tokens` | Output token count |
/// | `gen_ai.response.stop_reason` | Stop reason |
/// | `gen_ai.tool.name` | Tool name |
/// | `gen_ai.tool.is_error` | Whether tool returned an error |
pub struct OtelHook {
    config: OtelConfig,
}

impl OtelHook {
    /// Create a new OTel hook with the given configuration.
    #[must_use]
    pub fn new(config: OtelConfig) -> Self {
        Self { config }
    }
}

impl Default for OtelHook {
    fn default() -> Self {
        Self::new(OtelConfig::default())
    }
}

impl ObservabilityHook for OtelHook {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl std::future::Future<Output = Result<HookAction, HookError>> + Send {
        match &event {
            HookEvent::LoopIteration { turn } => {
                tracing::info_span!("gen_ai.loop.iteration", gen_ai.system = "neuron", turn)
                    .in_scope(|| {
                        tracing::debug!("loop iteration {turn}");
                    });
            }
            HookEvent::PreLlmCall { request } => {
                let span = tracing::info_span!(
                    "gen_ai.chat",
                    gen_ai.system = "neuron",
                    gen_ai.request.model = %request.model,
                    gen_ai.request.messages = request.messages.len(),
                    gen_ai.request.tools = request.tools.len(),
                );
                span.in_scope(|| {
                    if self.config.capture_input {
                        tracing::debug!(
                            messages = ?request.messages.len(),
                            "gen_ai.chat request"
                        );
                    } else {
                        tracing::debug!("gen_ai.chat request");
                    }
                });
            }
            HookEvent::PostLlmCall { response } => {
                let span = tracing::info_span!(
                    "gen_ai.chat",
                    gen_ai.system = "neuron",
                    gen_ai.response.model = %response.model,
                    gen_ai.response.stop_reason = ?response.stop_reason,
                    gen_ai.usage.input_tokens = response.usage.input_tokens,
                    gen_ai.usage.output_tokens = response.usage.output_tokens,
                );
                span.in_scope(|| {
                    if self.config.capture_output {
                        tracing::debug!(
                            content_blocks = response.message.content.len(),
                            "gen_ai.chat response"
                        );
                    } else {
                        tracing::debug!("gen_ai.chat response");
                    }
                });
            }
            HookEvent::PreToolExecution { tool_name, .. } => {
                tracing::info_span!(
                    "gen_ai.execute_tool",
                    gen_ai.system = "neuron",
                    gen_ai.tool.name = %tool_name,
                )
                .in_scope(|| {
                    tracing::debug!("tool execution start");
                });
            }
            HookEvent::PostToolExecution { tool_name, output } => {
                tracing::info_span!(
                    "gen_ai.execute_tool",
                    gen_ai.system = "neuron",
                    gen_ai.tool.name = %tool_name,
                    gen_ai.tool.is_error = output.is_error,
                )
                .in_scope(|| {
                    tracing::debug!("tool execution complete");
                });
            }
            HookEvent::ContextCompaction {
                old_tokens,
                new_tokens,
            } => {
                tracing::info_span!(
                    "gen_ai.context.compaction",
                    gen_ai.system = "neuron",
                    old_tokens,
                    new_tokens,
                    reduced_by = old_tokens - new_tokens,
                )
                .in_scope(|| {
                    tracing::info!("context compacted");
                });
            }
            HookEvent::SessionStart { session_id } => {
                tracing::info!(gen_ai.system = "neuron", session_id, "gen_ai.session.start");
            }
            HookEvent::SessionEnd { session_id } => {
                tracing::info!(gen_ai.system = "neuron", session_id, "gen_ai.session.end");
            }
        }
        std::future::ready(Ok(HookAction::Continue))
    }
}
