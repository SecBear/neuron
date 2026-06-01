//! Convenience trait for operators that compute a result synchronously
//! (no incremental streaming, no sub-dispatches).
//!
//! [`SyncOperator`] wraps the two-step `handle` → `collect` pattern into a
//! single `execute` method. [`SyncOperatorAdapter`] bridges any [`SyncOperator`]
//! into the full [`layer0::operator::Operator`] protocol, which emits a single
//! `Completed` event automatically.
//!
//! # When to use this
//!
//! - Tools: search, code execution, database lookup, etc.
//! - Prompt generators and formatters.
//! - Any operator whose full output is available before it returns.
//!
//! Operators that emit progress events or delegate to child dispatches
//! should implement [`layer0::operator::Operator`] directly.
//!
//! # Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use skg_context_engine::sync_operator::{SyncOperator, SyncOperatorAdapter};
//! use layer0::operator::Operator;
//!
//! let op: Arc<dyn Operator> = Arc::new(SyncOperatorAdapter(MyTool));
//! ```

use async_trait::async_trait;
use layer0::capability::CapabilityDescriptor;
use layer0::dispatch::DispatchHandle;
use layer0::dispatch_context::DispatchContext;
use layer0::error::ProtocolError;
use layer0::operator::{OperatorInput, OperatorOutput, completed_handle};

/// Convenience trait for operators that produce a single terminal output.
///
/// Implement `execute` to compute and return your result. Wrap with
/// [`SyncOperatorAdapter`] to get a full [`layer0::operator::Operator`]
/// implementation that handles the streaming dispatch protocol automatically.
///
/// # Error semantics
///
/// Errors returned from `execute` propagate as `Err` from
/// [`Operator::handle`](layer0::operator::Operator::handle), **not** as a
/// `DispatchEvent::Failed` event. This matches the documented contract:
/// `handle` returns `Err` when the operator cannot begin processing;
/// errors during processing go through the event stream. Since
/// `SyncOperator` has no event stream, all errors are "cannot begin."
#[async_trait]
pub trait SyncOperator: Send + Sync {
    /// Describes this operator's capabilities for discovery and routing.
    fn descriptor(&self) -> CapabilityDescriptor;

    /// Execute and return a terminal output.
    ///
    /// # Errors
    ///
    /// Return `Err` for any failure — the error propagates directly from
    /// [`Operator::handle`](layer0::operator::Operator::handle).
    async fn execute(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<OperatorOutput, ProtocolError>;
}

/// Adapts any [`SyncOperator`] into a full [`layer0::operator::Operator`].
///
/// The inner type is `pub` so callers can access it if needed (e.g., to
/// recover the original value or call `SyncOperator` methods directly).
///
/// # Example
///
/// ```rust,ignore
/// let op: Arc<dyn Operator> = Arc::new(SyncOperatorAdapter(MyTool));
/// ```
pub struct SyncOperatorAdapter<T: SyncOperator>(
    /// The wrapped [`SyncOperator`] implementation.
    pub T,
);

#[async_trait]
impl<T: SyncOperator + 'static> layer0::operator::Operator for SyncOperatorAdapter<T> {
    fn descriptor(&self) -> CapabilityDescriptor {
        self.0.descriptor()
    }

    async fn handle(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<DispatchHandle, ProtocolError> {
        let output = self.0.execute(input, ctx).await?;
        Ok(completed_handle(ctx.dispatch_id.clone(), output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer0::capability::{
        ApprovalFacts, AuthFacts, CapabilityId, CapabilityKind, ExecutionClass, SchedulingFacts,
    };
    use layer0::content::Content;
    use layer0::dispatch::DispatchEvent;
    use layer0::operator::{Operator, Outcome, TerminalOutcome, TriggerType};
    use layer0::{DispatchContext, DispatchId, OperatorId};
    use std::sync::Arc;

    // ── test doubles ─────────────────────────────────────────────────────────

    struct EchoSyncOp;

    #[async_trait]
    impl SyncOperator for EchoSyncOp {
        fn descriptor(&self) -> CapabilityDescriptor {
            CapabilityDescriptor::new(
                CapabilityId::new("test.echo"),
                CapabilityKind::Tool,
                "echo",
                "Echoes input",
                SchedulingFacts::new(ExecutionClass::Shared, false, false, false, None),
                ApprovalFacts::None,
                AuthFacts::Open,
            )
        }

        async fn execute(
            &self,
            input: OperatorInput,
            _ctx: &DispatchContext,
        ) -> Result<OperatorOutput, ProtocolError> {
            Ok(OperatorOutput::new(
                input.message,
                Outcome::Terminal {
                    terminal: TerminalOutcome::Completed,
                },
            ))
        }
    }

    // Fails immediately so we can verify error propagation.
    struct FailingSyncOp;

    #[async_trait]
    impl SyncOperator for FailingSyncOp {
        fn descriptor(&self) -> CapabilityDescriptor {
            CapabilityDescriptor::new(
                CapabilityId::new("test.fail"),
                CapabilityKind::Tool,
                "fail",
                "Always fails",
                SchedulingFacts::new(ExecutionClass::Shared, false, false, false, None),
                ApprovalFacts::None,
                AuthFacts::Open,
            )
        }

        async fn execute(
            &self,
            _input: OperatorInput,
            _ctx: &DispatchContext,
        ) -> Result<OperatorOutput, ProtocolError> {
            Err(ProtocolError::internal("deliberate failure"))
        }
    }

    fn test_ctx() -> DispatchContext {
        DispatchContext::new(DispatchId::new("dispatch-1"), OperatorId::new("op-echo"))
    }

    // ── tests ────────────────────────────────────────────────────────────────

    /// Compile-time proof: a SyncOperator wrapped in SyncOperatorAdapter is
    /// usable as Arc<dyn Operator>.
    #[test]
    fn sync_operator_adapter_is_dyn_operator() {
        let _: Arc<dyn Operator> = Arc::new(SyncOperatorAdapter(EchoSyncOp));
    }

    /// The handle emits a single Completed event carrying the echoed output.
    #[tokio::test]
    async fn handle_returns_completed_with_correct_output() {
        let op = SyncOperatorAdapter(EchoSyncOp);
        let input = OperatorInput::new(Content::text("hello world"), TriggerType::User);
        let ctx = test_ctx();

        let handle = op.handle(input, &ctx).await.expect("handle should succeed");

        let output = handle.collect().await.expect("collect should succeed");

        assert!(
            matches!(
                output.outcome,
                Outcome::Terminal {
                    terminal: TerminalOutcome::Completed,
                }
            ),
            "expected Completed outcome, got {:?}",
            output.outcome
        );
        assert_eq!(
            output.message,
            Content::text("hello world"),
            "output message must match input"
        );
    }

    /// Errors from execute() propagate as Err from handle(), not as Failed events.
    #[tokio::test]
    async fn execute_error_propagates_from_handle() {
        let op = SyncOperatorAdapter(FailingSyncOp);
        let input = OperatorInput::new(Content::text("trigger"), TriggerType::User);
        let ctx = test_ctx();

        let result = op.handle(input, &ctx).await;
        assert!(
            result.is_err(),
            "handle must return Err when execute fails, not Ok(handle)"
        );
    }

    /// Verify the Completed event contains the right payload when received via
    /// the event stream rather than collect().
    #[tokio::test]
    async fn completed_event_carries_output() {
        let op = SyncOperatorAdapter(EchoSyncOp);
        let input = OperatorInput::new(Content::text("streaming check"), TriggerType::Task);
        let ctx = test_ctx();

        let mut handle = op.handle(input, &ctx).await.expect("handle ok");
        let event = handle.recv().await.expect("must receive one event");

        match event {
            DispatchEvent::Completed { output } => {
                assert_eq!(output.message, Content::text("streaming check"));
            }
            other => panic!("expected Completed event, got {other:?}"),
        }

        // No further events after Completed.
        assert!(
            handle.recv().await.is_none(),
            "no events should follow Completed"
        );
    }
}
