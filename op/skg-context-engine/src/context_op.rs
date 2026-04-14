//! Composable, event-driven context transformation operations.
//!
//! [`ContextOp`] is the unit of context policy: an async transform gated by
//! a [`Trigger`] (deciding when it runs) and an [`apply`](ContextOp::apply)
//! body (mutating [`Context`] and returning an [`OpResult`] directing the
//! loop).
//!
//! Build ops with the fluent [`on`] builder for one-offs, or implement
//! [`ContextOp`] directly for named, reusable policy types.
//!
//! [`ErasedContextOp`] is the object-safe companion used by the pipeline
//! to store heterogeneous ops in a `Vec<Box<dyn ErasedContextOp>>`. A
//! blanket impl covers all [`ContextOp`] implementors automatically.

use crate::agent_event::{AgentEvent, EventKind};
use crate::context::Context;
use layer0::DispatchContext;
use layer0::wait::WaitReason;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

// ── OpResult ─────────────────────────────────────────────────────────────────

/// The outcome returned by a [`ContextOp`] after applying its transformation.
///
/// The pipeline runner inspects this value to decide how to proceed. The first
/// non-[`Continue`](OpResult::Continue) result from any op in the sequence
/// wins.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum OpResult {
    /// Context was (optionally) mutated; continue to the next op.
    Continue,
    /// Stop the agent loop immediately with the given reason string.
    Halt(String),
    /// Suspend the agent loop and wait for the given external condition.
    Suspend(WaitReason),
    /// This op does not apply to the current event; skip and continue.
    Skip,
}

// ── Trigger ───────────────────────────────────────────────────────────────────

/// A composable predicate that decides whether a [`ContextOp`] runs.
///
/// Triggers are evaluated against the current [`AgentEvent`] and [`Context`]
/// before `apply` is called. They compose: [`All`](Trigger::All) is logical
/// AND, [`Any`](Trigger::Any) is logical OR.
///
/// The [`When`](Trigger::When) variant accepts an arbitrary closure for cases
/// not covered by the structured variants.
pub enum Trigger {
    /// Matches when the event's [`kind`](AgentEvent::kind) equals the given
    /// [`EventKind`].
    Event(EventKind),
    /// Matches when **all** sub-triggers match (logical AND).
    All(Vec<Trigger>),
    /// Matches when **any** sub-trigger matches (logical OR).
    Any(Vec<Trigger>),
    /// Matches when the closure returns `true` for the event and context.
    When(Arc<dyn Fn(&AgentEvent, &Context) -> bool + Send + Sync>),
    /// Always matches, regardless of event or context.
    Always,
}

impl Clone for Trigger {
    fn clone(&self) -> Self {
        match self {
            Self::Event(k) => Self::Event(*k),
            Self::All(v) => Self::All(v.clone()),
            Self::Any(v) => Self::Any(v.clone()),
            Self::When(f) => Self::When(Arc::clone(f)),
            Self::Always => Self::Always,
        }
    }
}

impl fmt::Debug for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Event(k) => f.debug_tuple("Event").field(k).finish(),
            Self::All(v) => f.debug_tuple("All").field(v).finish(),
            Self::Any(v) => f.debug_tuple("Any").field(v).finish(),
            Self::When(_) => f.debug_tuple("When").field(&"...").finish(),
            Self::Always => write!(f, "Always"),
        }
    }
}

impl Trigger {
    /// Return `true` if this trigger matches the given `event` and `ctx`.
    ///
    /// Evaluation is short-circuit: [`All`](Trigger::All) stops at the first
    /// non-matching sub-trigger; [`Any`](Trigger::Any) stops at the first
    /// matching one.
    pub fn matches(&self, event: &AgentEvent, ctx: &Context) -> bool {
        match self {
            Self::Event(kind) => event.kind() == *kind,
            Self::All(triggers) => triggers.iter().all(|t| t.matches(event, ctx)),
            Self::Any(triggers) => triggers.iter().any(|t| t.matches(event, ctx)),
            Self::When(f) => f(event, ctx),
            Self::Always => true,
        }
    }
}

// ── ContextOp ─────────────────────────────────────────────────────────────────

/// A composable, event-driven context transformation.
///
/// Implement this trait for named, reusable ops (budget guards, compaction
/// policies, telemetry hooks). Use the [`on`] builder for anonymous one-offs.
///
/// The pipeline evaluates each registered op in order, calling `apply` only
/// when the op's [`trigger`](ContextOp::trigger) matches the current event.
#[async_trait::async_trait]
pub trait ContextOp: Send + Sync {
    /// The predicate that selects which events this op handles.
    fn trigger(&self) -> Trigger;

    /// Apply a context transformation in response to `event`.
    ///
    /// Mutations to `ctx` are visible to all subsequent ops regardless of the
    /// returned [`OpResult`]. The first [`Halt`](OpResult::Halt) or
    /// [`Suspend`](OpResult::Suspend) result stops further processing.
    async fn apply(
        &self,
        event: &AgentEvent,
        ctx: &mut Context,
        dispatch_ctx: &DispatchContext,
    ) -> OpResult;

    /// Human-readable name for tracing and diagnostics.
    ///
    /// Defaults to the Rust type name of the implementing type.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

// ── ErasedContextOp ───────────────────────────────────────────────────────────

/// Object-safe companion to [`ContextOp`] for heterogeneous pipeline storage.
///
/// Because [`ContextOp::apply`] is `async` (and therefore not object-safe),
/// this trait erases the future type behind a `Pin<Box<dyn Future>>` so ops
/// can be stored in a `Vec<Box<dyn ErasedContextOp>>`.
///
/// A blanket `impl<T: ContextOp> ErasedContextOp for T` is provided — you
/// never implement this trait directly.
pub trait ErasedContextOp: Send + Sync {
    /// Return the trigger predicate for this op.
    fn trigger(&self) -> Trigger;

    /// Apply the context transformation, returning a pinned boxed future.
    fn apply_erased<'a>(
        &'a self,
        event: &'a AgentEvent,
        ctx: &'a mut Context,
        dispatch_ctx: &'a DispatchContext,
    ) -> Pin<Box<dyn Future<Output = OpResult> + Send + 'a>>;

    /// Human-readable name for tracing and diagnostics.
    fn name(&self) -> &str;
}

impl<T: ContextOp> ErasedContextOp for T {
    fn trigger(&self) -> Trigger {
        ContextOp::trigger(self)
    }

    fn apply_erased<'a>(
        &'a self,
        event: &'a AgentEvent,
        ctx: &'a mut Context,
        dispatch_ctx: &'a DispatchContext,
    ) -> Pin<Box<dyn Future<Output = OpResult> + Send + 'a>> {
        Box::pin(async move { self.apply(event, ctx, dispatch_ctx).await })
    }

    fn name(&self) -> &str {
        ContextOp::name(self)
    }
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Fluent builder for anonymous [`ContextOp`]s.
///
/// Obtain via [`on`]; complete with [`apply`](ContextOpBuilder::apply).
pub struct ContextOpBuilder {
    trigger: Trigger,
}

/// Begin building a [`ContextOp`] that fires on the given [`EventKind`].
///
/// Chain [`when`](ContextOpBuilder::when) to add predicates, then call
/// [`apply`](ContextOpBuilder::apply) to attach the async body.
///
/// ```ignore
/// let op = on(EventKind::BeforeInference)
///     .apply(|_, ctx, _| async { OpResult::Continue });
/// ```
pub fn on(kind: EventKind) -> ContextOpBuilder {
    ContextOpBuilder {
        trigger: Trigger::Event(kind),
    }
}

impl ContextOpBuilder {
    /// Narrow the trigger with an additional predicate.
    ///
    /// The op only runs when both the base [`EventKind`] **and** `f` match.
    /// Multiple calls to `when` further narrow with logical AND.
    pub fn when(
        self,
        f: impl Fn(&AgentEvent, &Context) -> bool + Send + Sync + 'static,
    ) -> Self {
        let existing = self.trigger;
        ContextOpBuilder {
            trigger: Trigger::All(vec![existing, Trigger::When(Arc::new(f))]),
        }
    }

    /// Attach an async closure as the op body, completing the builder.
    ///
    /// The returned value implements [`ContextOp`] and can be registered
    /// with a pipeline.
    ///
    /// The closure must return a `'static` future. Closures that need to
    /// borrow `event`, `ctx`, or `dispatch_ctx` within the async block must
    /// copy or clone the data they need before returning the future.
    pub fn apply<F, Fut>(self, f: F) -> impl ContextOp
    where
        F: Fn(&AgentEvent, &mut Context, &DispatchContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = OpResult> + Send + 'static,
    {
        ClosureContextOp {
            trigger: self.trigger,
            f,
        }
    }
}

// ── ClosureContextOp (private) ────────────────────────────────────────────────

struct ClosureContextOp<F> {
    trigger: Trigger,
    f: F,
}

#[async_trait::async_trait]
impl<F, Fut> ContextOp for ClosureContextOp<F>
where
    F: Fn(&AgentEvent, &mut Context, &DispatchContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = OpResult> + Send + 'static,
{
    fn trigger(&self) -> Trigger {
        self.trigger.clone()
    }

    async fn apply(
        &self,
        event: &AgentEvent,
        ctx: &mut Context,
        dispatch_ctx: &DispatchContext,
    ) -> OpResult {
        (self.f)(event, ctx, dispatch_ctx).await
    }

    fn name(&self) -> &str {
        "closure_context_op"
    }
}
