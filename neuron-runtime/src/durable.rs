//! Durable execution contexts for local development and production engines.
//!
//! [`LocalDurableContext`] is a passthrough implementation for local dev/testing
//! that calls the provider and tools directly without journaling.
//!
//! For production durable execution (Temporal, Restate, Inngest), see the
//! documentation at the bottom of this module for how to implement
//! [`DurableContext`] with those SDKs.

use std::sync::Arc;
use std::time::Duration;

use neuron_tool::ToolRegistry;
use neuron_types::{
    ActivityOptions, CompletionRequest, CompletionResponse, DurableContext, DurableError, Provider,
    ToolContext, ToolOutput, WasmCompatSend,
};

/// A passthrough durable context for local development and testing.
///
/// Executes LLM calls and tool calls directly without journaling.
/// No crash recovery, no replay — just direct passthrough.
///
/// This is the default when you do not need durable execution but want
/// a concrete [`DurableContext`] implementation.
pub struct LocalDurableContext<P: Provider> {
    provider: Arc<P>,
    tools: Arc<ToolRegistry>,
}

impl<P: Provider> LocalDurableContext<P> {
    /// Create a new local durable context.
    #[must_use]
    pub fn new(provider: Arc<P>, tools: Arc<ToolRegistry>) -> Self {
        Self { provider, tools }
    }
}

impl<P: Provider> DurableContext for LocalDurableContext<P> {
    async fn execute_llm_call(
        &self,
        request: CompletionRequest,
        _options: ActivityOptions,
    ) -> Result<CompletionResponse, DurableError> {
        self.provider
            .complete(request)
            .await
            .map_err(|e| DurableError::ActivityFailed(e.to_string()))
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
        _options: ActivityOptions,
    ) -> Result<ToolOutput, DurableError> {
        self.tools
            .execute(tool_name, input, ctx)
            .await
            .map_err(|e| DurableError::ActivityFailed(e.to_string()))
    }

    async fn wait_for_signal<T: serde::de::DeserializeOwned + WasmCompatSend>(
        &self,
        _signal_name: &str,
        timeout: Duration,
    ) -> Result<Option<T>, DurableError> {
        // Local context: no signals, just wait for the timeout
        tokio::time::sleep(timeout).await;
        Ok(None)
    }

    fn should_continue_as_new(&self) -> bool {
        false
    }

    async fn continue_as_new(
        &self,
        _state: serde_json::Value,
    ) -> Result<(), DurableError> {
        // Local context: continue-as-new is a no-op
        Ok(())
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }

    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }
}

// =============================================================================
// Production durable execution implementations
// =============================================================================
//
// ## Temporal
//
// To implement `DurableContext` for Temporal, add `temporal-sdk` as a dependency
// (feature-gated) and wrap the Temporal workflow context:
//
// ```ignore
// use temporal_sdk::WfContext;
//
// pub struct TemporalDurableContext {
//     ctx: WfContext,
//     tools: Arc<ToolRegistry>,
// }
//
// impl DurableContext for TemporalDurableContext {
//     async fn execute_llm_call(
//         &self,
//         request: CompletionRequest,
//         options: ActivityOptions,
//     ) -> Result<CompletionResponse, DurableError> {
//         let input = serde_json::to_string(&request)
//             .map_err(|e| DurableError::ActivityFailed(e.to_string()))?;
//         let result = self.ctx
//             .activity(ActivityOptions {
//                 activity_type: "llm_call".to_string(),
//                 start_to_close_timeout: Some(options.start_to_close_timeout),
//                 heartbeat_timeout: options.heartbeat_timeout,
//                 retry_policy: options.retry_policy.map(convert_retry_policy),
//                 input: vec![input.into()],
//                 ..Default::default()
//             })
//             .await
//             .map_err(|e| DurableError::ActivityFailed(e.to_string()))?;
//         serde_json::from_slice(&result.result)
//             .map_err(|e| DurableError::ActivityFailed(e.to_string()))
//     }
//
//     // ... similar for execute_tool, wait_for_signal, etc.
//
//     fn should_continue_as_new(&self) -> bool {
//         self.ctx.get_info().is_continue_as_new_suggested()
//     }
//
//     async fn sleep(&self, duration: Duration) {
//         self.ctx.timer(duration).await;
//     }
//
//     fn now(&self) -> chrono::DateTime<chrono::Utc> {
//         // Temporal provides deterministic time during replay
//         self.ctx.workflow_time()
//     }
// }
// ```
//
// ## Restate
//
// To implement `DurableContext` for Restate, add `restate-sdk` as a dependency
// (feature-gated) and wrap the Restate context:
//
// ```ignore
// use restate_sdk::context::Context;
//
// pub struct RestateDurableContext {
//     ctx: Context,
//     tools: Arc<ToolRegistry>,
// }
//
// impl DurableContext for RestateDurableContext {
//     async fn execute_llm_call(
//         &self,
//         request: CompletionRequest,
//         _options: ActivityOptions,
//     ) -> Result<CompletionResponse, DurableError> {
//         self.ctx
//             .run("llm_call", || async {
//                 // Direct LLM call here — Restate journals the result
//                 provider.complete(request).await
//             })
//             .await
//             .map_err(|e| DurableError::ActivityFailed(e.to_string()))
//     }
//
//     // ... similar for execute_tool, etc.
//
//     async fn sleep(&self, duration: Duration) {
//         self.ctx.sleep(duration).await;
//     }
// }
// ```
