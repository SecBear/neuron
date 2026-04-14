//! Reactive event-driven pipeline that evaluates [`ContextOp`]s against [`AgentEvent`]s.
//!
//! Operations are evaluated in registration order. Only ops whose [`Trigger`]
//! matches the event are invoked. The first [`OpResult::Halt`] or
//! [`OpResult::Suspend`] short-circuits the chain.
//!
//! ## Usage
//!
//! ```ignore
//! let pipeline = ReactivePipeline::new()
//!     .add(on(EventKind::BeforeInference).apply(|_, ctx, _| async {
//!         ctx.inject_system("You are a helpful assistant.");
//!         OpResult::Continue
//!     }));
//!
//! let result = pipeline.emit(&AgentEvent::BeforeInference, &mut ctx, &dctx).await;
//! ```

use crate::agent_event::AgentEvent;
use crate::context::Context;
use crate::context_op::{ContextOp, ErasedContextOp, OpResult};
use layer0::dispatch_context::DispatchContext;

/// Reactive event-driven pipeline.
///
/// Holds an ordered list of [`ErasedContextOp`]s and evaluates them for
/// each [`AgentEvent`]. Only ops whose [`Trigger`](crate::Trigger) matches
/// are invoked. The first [`OpResult::Halt`] or [`OpResult::Suspend`]
/// short-circuits the chain; all other outcomes advance to the next op.
///
/// ## Builder pattern
///
/// ```ignore
/// let pipeline = ReactivePipeline::new()
///     .add(on(EventKind::LoopStarted).apply(inject_system_prompt))
///     .add(on(EventKind::BeforeInference).apply(check_budget));
/// ```
pub struct ReactivePipeline {
    ops: Vec<Box<dyn ErasedContextOp>>,
}

impl ReactivePipeline {
    /// Create an empty pipeline with no registered ops.
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    #[allow(clippy::should_implement_trait)]
    /// Add a context op, consuming and returning `self` for chaining.
    pub fn add(mut self, op: impl ContextOp + 'static) -> Self {
        self.ops.push(Box::new(op));
        self
    }

    /// Add a pre-boxed context op.
    pub fn add_boxed(mut self, op: Box<dyn ErasedContextOp>) -> Self {
        self.ops.push(op);
        self
    }

    /// Evaluate all ops whose trigger matches `event`.
    ///
    /// Ops run in registration order. [`OpResult::Continue`] and
    /// [`OpResult::Skip`] both advance to the next op.
    /// [`OpResult::Halt`] or [`OpResult::Suspend`] stop the chain and are
    /// returned immediately.
    ///
    /// Returns [`OpResult::Continue`] when the chain completes without a halt
    /// or suspend.
    pub async fn emit(
        &self,
        event: &AgentEvent,
        ctx: &mut Context,
        dispatch_ctx: &DispatchContext,
    ) -> OpResult {
        for op in &self.ops {
            if op.trigger().matches(event, ctx) {
                match op.apply_erased(event, ctx, dispatch_ctx).await {
                    OpResult::Continue | OpResult::Skip => {}
                    terminal => return terminal,
                }
            }
        }
        OpResult::Continue
    }

    /// Number of registered ops.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Returns `true` if no ops are registered.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl Default for ReactivePipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_event::EventKind;
    use crate::context_op::{Trigger, on};
    use layer0::id::{DispatchId, OperatorId};
    use layer0::wait::WaitReason;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn test_dctx() -> DispatchContext {
        DispatchContext::new(DispatchId::new("test"), OperatorId::new("test"))
    }

    #[tokio::test]
    async fn empty_pipeline_returns_continue() {
        let pipeline = ReactivePipeline::new();
        let mut ctx = Context::new();
        let dctx = test_dctx();
        let result = pipeline
            .emit(&AgentEvent::LoopStarted, &mut ctx, &dctx)
            .await;
        assert!(matches!(result, OpResult::Continue));
    }

    #[tokio::test]
    async fn matching_ops_run_in_order() {
        let counter = Arc::new(AtomicU32::new(0));
        let c1 = counter.clone();
        let c2 = counter.clone();

        let pipeline = ReactivePipeline::new()
            .add(on(EventKind::LoopStarted).apply(move |_, _, _| {
                c1.fetch_add(10, Ordering::SeqCst);
                async { OpResult::Continue }
            }))
            .add(on(EventKind::LoopStarted).apply(move |_, _, _| {
                // First op must have run before us.
                assert_eq!(c2.load(Ordering::SeqCst), 10, "second op runs after first");
                c2.fetch_add(1, Ordering::SeqCst);
                async { OpResult::Continue }
            }));

        let mut ctx = Context::new();
        let dctx = test_dctx();
        let result = pipeline
            .emit(&AgentEvent::LoopStarted, &mut ctx, &dctx)
            .await;
        assert!(matches!(result, OpResult::Continue));
        assert_eq!(counter.load(Ordering::SeqCst), 11);
    }

    #[tokio::test]
    async fn non_matching_ops_are_skipped() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        // Registered for BeforeInference only.
        let pipeline =
            ReactivePipeline::new().add(on(EventKind::BeforeInference).apply(move |_, _, _| {
                c.fetch_add(1, Ordering::SeqCst);
                async { OpResult::Continue }
            }));

        let mut ctx = Context::new();
        let dctx = test_dctx();
        // LoopStarted must not trigger a BeforeInference op.
        let result = pipeline
            .emit(&AgentEvent::LoopStarted, &mut ctx, &dctx)
            .await;
        assert!(matches!(result, OpResult::Continue));
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn halt_stops_chain() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        let pipeline = ReactivePipeline::new()
            .add(
                on(EventKind::LoopStarted)
                    .apply(|_, _, _| async { OpResult::Halt("test halt".into()) }),
            )
            // This op must NOT run after the halt.
            .add(on(EventKind::LoopStarted).apply(move |_, _, _| {
                c.fetch_add(1, Ordering::SeqCst);
                async { OpResult::Continue }
            }));

        let mut ctx = Context::new();
        let dctx = test_dctx();
        let result = pipeline
            .emit(&AgentEvent::LoopStarted, &mut ctx, &dctx)
            .await;
        assert!(matches!(result, OpResult::Halt(_)));
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn suspend_stops_chain() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        let pipeline = ReactivePipeline::new()
            .add(
                on(EventKind::LoopStarted)
                    .apply(|_, _, _| async { OpResult::Suspend(WaitReason::Approval) }),
            )
            // This op must NOT run after the suspend.
            .add(on(EventKind::LoopStarted).apply(move |_, _, _| {
                c.fetch_add(1, Ordering::SeqCst);
                async { OpResult::Continue }
            }));

        let mut ctx = Context::new();
        let dctx = test_dctx();
        let result = pipeline
            .emit(&AgentEvent::LoopStarted, &mut ctx, &dctx)
            .await;
        assert!(matches!(result, OpResult::Suspend(WaitReason::Approval)));
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn skip_is_treated_like_continue() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        let pipeline = ReactivePipeline::new()
            .add(on(EventKind::LoopStarted).apply(|_, _, _| async { OpResult::Skip }))
            // Skip must not stop the chain — this op must still run.
            .add(on(EventKind::LoopStarted).apply(move |_, _, _| {
                c.fetch_add(1, Ordering::SeqCst);
                async { OpResult::Continue }
            }));

        let mut ctx = Context::new();
        let dctx = test_dctx();
        let result = pipeline
            .emit(&AgentEvent::LoopStarted, &mut ctx, &dctx)
            .await;
        assert!(matches!(result, OpResult::Continue));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn event_kind_trigger_fires_only_on_before_inference() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        let pipeline =
            ReactivePipeline::new().add(on(EventKind::BeforeInference).apply(move |_, _, _| {
                c.fetch_add(1, Ordering::SeqCst);
                async { OpResult::Continue }
            }));

        let mut ctx = Context::new();
        let dctx = test_dctx();

        // LoopStarted must not trigger a BeforeInference op.
        pipeline
            .emit(&AgentEvent::LoopStarted, &mut ctx, &dctx)
            .await;
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // BeforeInference must trigger it.
        pipeline
            .emit(&AgentEvent::BeforeInference, &mut ctx, &dctx)
            .await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn trigger_always_fires_on_every_event() {
        struct AlwaysOp {
            counter: Arc<AtomicU32>,
        }

        #[async_trait::async_trait]
        impl ContextOp for AlwaysOp {
            fn trigger(&self) -> Trigger {
                Trigger::Always
            }

            async fn apply(
                &self,
                _event: &AgentEvent,
                _ctx: &mut Context,
                _dispatch_ctx: &DispatchContext,
            ) -> OpResult {
                self.counter.fetch_add(1, Ordering::SeqCst);
                OpResult::Continue
            }
        }

        let counter = Arc::new(AtomicU32::new(0));
        let pipeline = ReactivePipeline::new().add(AlwaysOp {
            counter: counter.clone(),
        });

        let mut ctx = Context::new();
        let dctx = test_dctx();

        pipeline
            .emit(&AgentEvent::LoopStarted, &mut ctx, &dctx)
            .await;
        pipeline
            .emit(&AgentEvent::BeforeInference, &mut ctx, &dctx)
            .await;

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
