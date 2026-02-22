//! Input and output guardrails with tripwire support.
//!
//! Guardrails check input before it reaches the LLM and output before it
//! reaches the user. A [`GuardrailResult::Tripwire`] halts execution
//! immediately, while [`GuardrailResult::Warn`] allows execution to continue
//! with a logged warning.

use std::future::Future;

use neuron_types::{WasmCompatSend, WasmCompatSync};

/// Result of a guardrail check.
#[derive(Debug, Clone)]
pub enum GuardrailResult {
    /// Input/output is acceptable.
    Pass,
    /// Immediately halt execution. The string explains why.
    Tripwire(String),
    /// Allow execution but log a warning. The string is the warning message.
    Warn(String),
}

impl GuardrailResult {
    /// Returns `true` if the result is [`GuardrailResult::Pass`].
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Returns `true` if the result is [`GuardrailResult::Tripwire`].
    #[must_use]
    pub fn is_tripwire(&self) -> bool {
        matches!(self, Self::Tripwire(_))
    }

    /// Returns `true` if the result is [`GuardrailResult::Warn`].
    #[must_use]
    pub fn is_warn(&self) -> bool {
        matches!(self, Self::Warn(_))
    }
}

/// Guardrail that checks input before it reaches the LLM.
///
/// # Example
///
/// ```ignore
/// use neuron_runtime::*;
///
/// struct NoSecrets;
/// impl InputGuardrail for NoSecrets {
///     fn check(&self, input: &str) -> impl Future<Output = GuardrailResult> + Send {
///         async move {
///             if input.contains("API_KEY") {
///                 GuardrailResult::Tripwire("Input contains API key".to_string())
///             } else {
///                 GuardrailResult::Pass
///             }
///         }
///     }
/// }
/// ```
pub trait InputGuardrail: WasmCompatSend + WasmCompatSync {
    /// Check the input text and return a guardrail result.
    fn check(&self, input: &str) -> impl Future<Output = GuardrailResult> + WasmCompatSend;
}

/// Guardrail that checks output before it reaches the user.
///
/// # Example
///
/// ```ignore
/// use neuron_runtime::*;
///
/// struct NoLeakedSecrets;
/// impl OutputGuardrail for NoLeakedSecrets {
///     fn check(&self, output: &str) -> impl Future<Output = GuardrailResult> + Send {
///         async move {
///             if output.contains("sk-") {
///                 GuardrailResult::Tripwire("Output contains secret key".to_string())
///             } else {
///                 GuardrailResult::Pass
///             }
///         }
///     }
/// }
/// ```
pub trait OutputGuardrail: WasmCompatSend + WasmCompatSync {
    /// Check the output text and return a guardrail result.
    fn check(&self, output: &str) -> impl Future<Output = GuardrailResult> + WasmCompatSend;
}

/// Run a sequence of input guardrails, returning the first non-Pass result.
///
/// Returns [`GuardrailResult::Pass`] if all guardrails pass.
pub async fn run_input_guardrails(
    guardrails: &[&dyn ErasedInputGuardrail],
    input: &str,
) -> GuardrailResult {
    for guardrail in guardrails {
        let result = guardrail.check_dyn(input).await;
        if !result.is_pass() {
            return result;
        }
    }
    GuardrailResult::Pass
}

/// Run a sequence of output guardrails, returning the first non-Pass result.
///
/// Returns [`GuardrailResult::Pass`] if all guardrails pass.
pub async fn run_output_guardrails(
    guardrails: &[&dyn ErasedOutputGuardrail],
    output: &str,
) -> GuardrailResult {
    for guardrail in guardrails {
        let result = guardrail.check_dyn(output).await;
        if !result.is_pass() {
            return result;
        }
    }
    GuardrailResult::Pass
}

// --- Type erasure for guardrails (RPITIT is not dyn-compatible) ---

/// Dyn-compatible wrapper for [`InputGuardrail`].
pub trait ErasedInputGuardrail: WasmCompatSend + WasmCompatSync {
    /// Check input, returning a boxed future.
    fn check_dyn<'a>(
        &'a self,
        input: &'a str,
    ) -> std::pin::Pin<Box<dyn Future<Output = GuardrailResult> + Send + 'a>>;
}

impl<T: InputGuardrail> ErasedInputGuardrail for T {
    fn check_dyn<'a>(
        &'a self,
        input: &'a str,
    ) -> std::pin::Pin<Box<dyn Future<Output = GuardrailResult> + Send + 'a>> {
        Box::pin(self.check(input))
    }
}

/// Dyn-compatible wrapper for [`OutputGuardrail`].
pub trait ErasedOutputGuardrail: WasmCompatSend + WasmCompatSync {
    /// Check output, returning a boxed future.
    fn check_dyn<'a>(
        &'a self,
        output: &'a str,
    ) -> std::pin::Pin<Box<dyn Future<Output = GuardrailResult> + Send + 'a>>;
}

impl<T: OutputGuardrail> ErasedOutputGuardrail for T {
    fn check_dyn<'a>(
        &'a self,
        output: &'a str,
    ) -> std::pin::Pin<Box<dyn Future<Output = GuardrailResult> + Send + 'a>> {
        Box::pin(self.check(output))
    }
}
