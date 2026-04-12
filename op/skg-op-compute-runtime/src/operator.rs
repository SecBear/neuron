//! Compute operator — prompts model for Python code, executes via runtime.

use async_trait::async_trait;
use layer0::content::{Content, ContentBlock};
use layer0::dispatch_context::DispatchContext;
use layer0::error::ProtocolError;
use layer0::id::SessionId;
use layer0::operator::{Operator, OperatorInput, OperatorOutput, Outcome, TerminalOutcome};
use skg_turn::infer::InferRequest;
use skg_turn::provider::Provider;
use std::sync::Arc;

use crate::{ComputeRuntime, ExecutionProfile};

/// Static configuration for a compute operator.
#[derive(Debug, Clone, Default)]
pub struct ComputeConfig {
    /// Execution profile applied to every runtime execution.
    pub profile: ExecutionProfile,
}

/// Minimal compute operator: ask model for a fenced Python block,
/// execute it in the compute runtime, and return the result.
#[derive(Debug)]
pub struct ComputeOperator<P, R> {
    provider: P,
    runtime: Arc<R>,
    config: ComputeConfig,
}

impl<P, R> ComputeOperator<P, R> {
    /// Create a new compute operator with a provider and compute runtime.
    pub fn new(provider: P, runtime: Arc<R>) -> Self {
        Self {
            provider,
            runtime,
            config: ComputeConfig::default(),
        }
    }

    /// Replace the operator configuration.
    pub fn with_config(mut self, config: ComputeConfig) -> Self {
        self.config = config;
        self
    }
}

impl<P, R> ComputeOperator<P, R>
where
    P: Provider,
    R: ComputeRuntime + 'static,
{
    fn system_prompt() -> &'static str {
        "You are a Python code generator. Given the user's request, return ONLY a single fenced code block containing valid Python. Use triple backticks with language tag 'python'. Do not include explanations or extra fences. A persistent Python session already provides helper functions such as final(...), note(...), capabilities(), and help_bindings(...); inspect capabilities when you need to discover available built-ins. End your program by calling final(...) when producing a structured answer, otherwise print to stdout."
    }

    fn extract_python(content: &Content) -> Option<String> {
        let s = content.as_text()?;
        let lower = s.to_ascii_lowercase();
        let start = if let Some(i) = lower.find("```python") {
            i
        } else if let Some(i) = lower.find("```py") {
            i
        } else {
            return None;
        };
        let after_fence = s[start..].find('\n').map(|o| start + o + 1)?;
        let close_rel = s[after_fence..].find("```")?;
        let close = after_fence + close_rel;
        Some(s[after_fence..close].to_string())
    }
}

#[async_trait]
impl<P, R> Operator for ComputeOperator<P, R>
where
    P: Provider + Send + Sync + 'static,
    R: ComputeRuntime + Send + Sync + 'static,
{
    #[tracing::instrument(skip_all, fields(trigger = ?input.trigger))]
    async fn execute(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<OperatorOutput, ProtocolError> {
        // 1) Call provider to get Python code
        let mut req = InferRequest::new(vec![layer0::context::Message::new(
            layer0::context::Role::User,
            input.message.clone(),
        )]);
        req = req.with_system(Self::system_prompt());
        let resp = self.provider.infer(req).await.map_err(|e| {
            if e.is_retryable() {
                ProtocolError::unavailable(e.to_string())
            } else {
                ProtocolError::internal(e.to_string())
            }
        })?;

        let code = Self::extract_python(&resp.content)
            .ok_or_else(|| ProtocolError::internal("model did not return a fenced python block"))?;

        // 2) Execute via runtime under current or per-dispatch fallback session.
        let session_id = input
            .session
            .unwrap_or_else(|| SessionId::new(format!("compute-{}", ctx.dispatch_id.as_str())));
        let report = self
            .runtime
            .exec(&session_id, &code, &self.config.profile)
            .await
            .map_err(|e| ProtocolError::internal(e.to_string()))?;

        // 3) Prefer structured final_result, else stdout.
        let message = if let Some(data) = report.final_result {
            Content::Blocks(vec![ContentBlock::Data {
                data,
                media_type: Some("application/json".into()),
            }])
        } else {
            Content::text(report.stdout)
        };

        let mut out = OperatorOutput::new(
            message,
            Outcome::Terminal {
                terminal: TerminalOutcome::Completed,
            },
        );
        // Minimal honest metadata — no turns/tools for this PoC.
        out.metadata.turns_used = 1;
        Ok(out)
    }
}
