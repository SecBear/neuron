//! Adapter that wraps guardrails as an [`ObservabilityHook`].
//!
//! [`GuardrailHook`] runs input guardrails on [`HookEvent::PreLlmCall`] and output
//! guardrails on [`HookEvent::PostLlmCall`], mapping [`GuardrailResult`] variants
//! to [`HookAction`] values.
//!
//! # Example
//!
//! ```ignore
//! use neuron_runtime::{GuardrailHook, InputGuardrail, OutputGuardrail, GuardrailResult};
//!
//! struct BlockSecrets;
//! impl InputGuardrail for BlockSecrets {
//!     fn check(&self, input: &str) -> impl Future<Output = GuardrailResult> + Send {
//!         async move {
//!             if input.contains("API_KEY") {
//!                 GuardrailResult::Tripwire("secret detected".to_string())
//!             } else {
//!                 GuardrailResult::Pass
//!             }
//!         }
//!     }
//! }
//!
//! let hook = GuardrailHook::new().input_guardrail(BlockSecrets);
//! // Use `hook` as an ObservabilityHook in the agent loop
//! ```

use std::sync::Arc;

use neuron_types::{
    ContentBlock, HookAction, HookError, HookEvent, ObservabilityHook, WasmCompatSend,
};

use crate::guardrail::{ErasedInputGuardrail, ErasedOutputGuardrail, GuardrailResult};

/// An [`ObservabilityHook`] that runs guardrails on LLM input and output.
///
/// Built with a builder pattern. Input guardrails fire on [`HookEvent::PreLlmCall`],
/// output guardrails fire on [`HookEvent::PostLlmCall`]. All other events pass
/// through with [`HookAction::Continue`].
///
/// # Guardrail result mapping
///
/// - [`GuardrailResult::Pass`] -> [`HookAction::Continue`]
/// - [`GuardrailResult::Tripwire`] -> [`HookAction::Terminate`] with the reason
/// - [`GuardrailResult::Warn`] -> logs a warning via [`tracing::warn!`] and continues
///
/// # Example
///
/// ```ignore
/// use neuron_runtime::{GuardrailHook, InputGuardrail, OutputGuardrail, GuardrailResult};
///
/// struct NoSecrets;
/// impl InputGuardrail for NoSecrets {
///     fn check(&self, input: &str) -> impl Future<Output = GuardrailResult> + Send {
///         async move {
///             if input.contains("sk-") {
///                 GuardrailResult::Tripwire("secret in input".to_string())
///             } else {
///                 GuardrailResult::Pass
///             }
///         }
///     }
/// }
///
/// let hook = GuardrailHook::new()
///     .input_guardrail(NoSecrets);
/// ```
pub struct GuardrailHook {
    input_guardrails: Vec<Arc<dyn ErasedInputGuardrail>>,
    output_guardrails: Vec<Arc<dyn ErasedOutputGuardrail>>,
}

impl GuardrailHook {
    /// Create an empty `GuardrailHook` with no guardrails.
    #[must_use]
    pub fn new() -> Self {
        Self {
            input_guardrails: Vec::new(),
            output_guardrails: Vec::new(),
        }
    }

    /// Add an input guardrail.
    ///
    /// Input guardrails run on [`HookEvent::PreLlmCall`], checking the last
    /// user message text in the request.
    #[must_use]
    pub fn input_guardrail<G>(mut self, guardrail: G) -> Self
    where
        G: ErasedInputGuardrail + 'static,
    {
        self.input_guardrails.push(Arc::new(guardrail));
        self
    }

    /// Add an output guardrail.
    ///
    /// Output guardrails run on [`HookEvent::PostLlmCall`], checking the
    /// assistant response text from the response message.
    #[must_use]
    pub fn output_guardrail<G>(mut self, guardrail: G) -> Self
    where
        G: ErasedOutputGuardrail + 'static,
    {
        self.output_guardrails.push(Arc::new(guardrail));
        self
    }
}

impl Default for GuardrailHook {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the text content from the last user message in the request's messages.
///
/// Returns an empty string if there are no user messages or no text blocks.
fn extract_last_user_text(messages: &[neuron_types::Message]) -> String {
    for message in messages.iter().rev() {
        if message.role == neuron_types::Role::User {
            let texts: Vec<&str> = message
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect();
            if !texts.is_empty() {
                return texts.join("\n");
            }
        }
    }
    String::new()
}

/// Extract text content from the assistant response message.
///
/// Returns an empty string if there are no text blocks.
fn extract_response_text(message: &neuron_types::Message) -> String {
    let texts: Vec<&str> = message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    texts.join("\n")
}

/// Map a [`GuardrailResult`] to a [`HookAction`], logging warnings as needed.
fn map_guardrail_result(result: GuardrailResult, direction: &str) -> HookAction {
    match result {
        GuardrailResult::Pass => HookAction::Continue,
        GuardrailResult::Tripwire(reason) => HookAction::Terminate { reason },
        GuardrailResult::Warn(reason) => {
            tracing::warn!("{direction} guardrail warning: {reason}");
            HookAction::Continue
        }
    }
}

impl ObservabilityHook for GuardrailHook {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + WasmCompatSend {
        // Capture references needed for the async block before moving into it.
        let input_guardrails = &self.input_guardrails;
        let output_guardrails = &self.output_guardrails;

        async move {
            match event {
                HookEvent::PreLlmCall { request } => {
                    if input_guardrails.is_empty() {
                        return Ok(HookAction::Continue);
                    }
                    let text = extract_last_user_text(&request.messages);
                    if text.is_empty() {
                        return Ok(HookAction::Continue);
                    }
                    for guardrail in input_guardrails {
                        let result = guardrail.check_dyn(&text).await;
                        if !result.is_pass() {
                            return Ok(map_guardrail_result(result, "input"));
                        }
                    }
                    Ok(HookAction::Continue)
                }
                HookEvent::PostLlmCall { response } => {
                    if output_guardrails.is_empty() {
                        return Ok(HookAction::Continue);
                    }
                    let text = extract_response_text(&response.message);
                    if text.is_empty() {
                        return Ok(HookAction::Continue);
                    }
                    for guardrail in output_guardrails {
                        let result = guardrail.check_dyn(&text).await;
                        if !result.is_pass() {
                            return Ok(map_guardrail_result(result, "output"));
                        }
                    }
                    Ok(HookAction::Continue)
                }
                _ => Ok(HookAction::Continue),
            }
        }
    }
}
