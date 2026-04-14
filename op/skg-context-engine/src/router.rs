//! [`Router`] — dispatches incoming calls to registered child [`Operator`]s.
//!
//! A `Router` holds a named set of operators and forwards each invocation to
//! the child whose [`OperatorId`] matches `ctx.operator_id`. It is itself an
//! [`Operator`], so routers compose: a router can be registered as a child of
//! another router.
//!
//! # Example
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use skg_context_engine::Router;
//! use layer0::{OperatorId, CapabilityId};
//!
//! let router = Router::new("my.router", "My Router")
//!     .route("tool.echo", Arc::new(EchoOperator));
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use layer0::capability::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind, ExecutionClass,
    SchedulingFacts,
};
use layer0::dispatch::DispatchHandle;
use layer0::dispatch_context::DispatchContext;
use layer0::error::ProtocolError;
use layer0::operator::{Operator, OperatorInput};
use layer0::OperatorId;

/// Routes operator invocations to registered child [`Operator`]s.
///
/// The router matches `ctx.operator_id` against its registry and delegates to
/// the corresponding child. If no match is found, it returns
/// [`ProtocolError::not_found`].
///
/// # Composability
///
/// `Router` implements [`Operator`], so it can be nested inside other routers
/// or any component that accepts an `Arc<dyn Operator>`.
pub struct Router {
    /// Registered child operators keyed by their [`OperatorId`].
    routes: HashMap<OperatorId, Arc<dyn Operator>>,
    /// This router's own capability descriptor.
    descriptor: CapabilityDescriptor,
}

impl Router {
    /// Create a new router with the given capability ID and display name.
    ///
    /// The router's descriptor uses [`CapabilityKind::Service`] and conservative
    /// defaults for scheduling (shared, ordering-insensitive, idempotent,
    /// interruptible). Adjust the descriptor after construction if your use-case
    /// requires different scheduling semantics.
    pub fn new(id: impl Into<CapabilityId>, name: impl Into<String>) -> Self {
        let name = name.into();
        let descriptor = CapabilityDescriptor::new(
            id,
            CapabilityKind::Service,
            name.clone(),
            format!("Router: {name}"),
            SchedulingFacts::new(ExecutionClass::Shared, false, true, true, None),
            ApprovalFacts::None,
            AuthFacts::Open,
        );
        Self {
            routes: HashMap::new(),
            descriptor,
        }
    }

    /// Register an operator under the given ID, returning `self` for chaining.
    pub fn route(mut self, id: impl Into<OperatorId>, op: Arc<dyn Operator>) -> Self {
        self.routes.insert(id.into(), op);
        self
    }

    /// Register an operator under the given ID (mutable borrow variant).
    pub fn register(&mut self, id: impl Into<OperatorId>, op: Arc<dyn Operator>) {
        self.routes.insert(id.into(), op);
    }

    /// Look up a registered operator by ID.
    pub fn get(&self, id: &OperatorId) -> Option<&Arc<dyn Operator>> {
        self.routes.get(id)
    }

    /// Collect the [`CapabilityDescriptor`] from every registered child.
    pub fn capabilities(&self) -> Vec<CapabilityDescriptor> {
        self.routes.values().map(|op| op.descriptor()).collect()
    }

    /// List all registered [`OperatorId`]s.
    pub fn operator_ids(&self) -> Vec<OperatorId> {
        self.routes.keys().cloned().collect()
    }

    /// Number of registered operators.
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Returns `true` when no operators are registered.
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

#[async_trait::async_trait]
impl Operator for Router {
    fn descriptor(&self) -> CapabilityDescriptor {
        self.descriptor.clone()
    }

    /// Dispatch the invocation to the child operator whose ID matches
    /// `ctx.operator_id`. Returns [`ProtocolError::not_found`] when no child
    /// is registered for that ID.
    async fn handle(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<DispatchHandle, ProtocolError> {
        match self.routes.get(&ctx.operator_id) {
            Some(op) => op.handle(input, ctx).await,
            None => Err(ProtocolError::not_found(format!(
                "operator not found: {}",
                ctx.operator_id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer0::capability::{CapabilityId, CapabilityKind};
    use layer0::content::Content;
    use layer0::dispatch::DispatchEvent;
    use layer0::operator::{Operator, OperatorInput, Outcome, TerminalOutcome, TriggerType};
    use layer0::test_utils::EchoOperator;
    use layer0::{DispatchContext, DispatchId};

    fn echo_op() -> Arc<dyn Operator> {
        Arc::new(EchoOperator)
    }

    fn dispatch_ctx(operator_id: &str) -> DispatchContext {
        DispatchContext::new(
            DispatchId::new("test-dispatch"),
            OperatorId::new(operator_id),
        )
    }

    // ── 1. Router with registered op routes correctly ─────────────────────────

    #[tokio::test]
    async fn routes_to_registered_operator() {
        let router = Router::new("router.test", "Test Router")
            .route("echo", echo_op());

        let input = OperatorInput::new(Content::text("hello"), TriggerType::User);
        let ctx = dispatch_ctx("echo");

        let handle = router
            .handle(input, &ctx)
            .await
            .expect("handle should succeed for registered op");

        let output = handle.collect().await.expect("collect ok");
        assert!(
            matches!(
                output.outcome,
                Outcome::Terminal {
                    terminal: TerminalOutcome::Completed,
                }
            ),
            "expected Completed, got {:?}",
            output.outcome
        );
        assert_eq!(output.message, Content::text("hello"));
    }

    // ── 2. Router returns NotFound for unregistered operator ──────────────────

    #[tokio::test]
    async fn returns_not_found_for_unknown_operator() {
        let router = Router::new("router.test", "Test Router");
        let input = OperatorInput::new(Content::text("oops"), TriggerType::User);
        let ctx = dispatch_ctx("missing.op");

        let err = router
            .handle(input, &ctx)
            .await
            .expect_err("should fail for unregistered op");

        assert_eq!(
            err.code,
            layer0::error::ErrorCode::NotFound,
            "expected NotFound error code"
        );
        assert!(
            err.message.contains("missing.op"),
            "error message should name the missing operator, got: {}",
            err.message
        );
    }

    // ── 3. capabilities() returns child descriptors ───────────────────────────

    #[test]
    fn capabilities_returns_child_descriptors() {
        let router = Router::new("router.test", "Test Router")
            .route("echo", echo_op());

        let caps = router.capabilities();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].id, CapabilityId::new("test.echo"));
        assert_eq!(caps[0].kind, CapabilityKind::Tool);
    }

    // ── 4. Builder pattern chains work ────────────────────────────────────────

    #[test]
    fn builder_chain_registers_multiple_operators() {
        let router = Router::new("router.multi", "Multi Router")
            .route("op.a", echo_op())
            .route("op.b", echo_op())
            .route("op.c", echo_op());

        assert_eq!(router.len(), 3);
        assert!(!router.is_empty());

        let ids: Vec<_> = {
            let mut v = router.operator_ids();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        assert_eq!(ids[0].as_str(), "op.a");
        assert_eq!(ids[1].as_str(), "op.b");
        assert_eq!(ids[2].as_str(), "op.c");
    }

    // ── 5. register() mutable variant works alongside builder ─────────────────

    #[test]
    fn register_mut_adds_operator() {
        let mut router = Router::new("router.mut", "Mut Router");
        assert!(router.is_empty());

        router.register("dyn.op", echo_op());
        assert_eq!(router.len(), 1);
        assert!(router.get(&OperatorId::new("dyn.op")).is_some());
    }

    // ── 6. Router descriptor uses CapabilityKind::Service ─────────────────────

    #[test]
    fn router_descriptor_is_service_kind() {
        let router = Router::new("my.router", "My Router");
        let desc = router.descriptor();
        assert_eq!(desc.id, CapabilityId::new("my.router"));
        assert_eq!(desc.kind, CapabilityKind::Service);
    }

    // ── 7. Router is itself an Arc<dyn Operator> ──────────────────────────────

    #[test]
    fn router_is_dyn_operator() {
        let _: Arc<dyn Operator> = Arc::new(Router::new("router.dyn", "Dyn Router"));
    }

    // ── 8. Routing dispatches event stream correctly ───────────────────────────

    #[tokio::test]
    async fn routed_event_stream_contains_completed() {
        let router = Router::new("router.stream", "Stream Router").route("echo", echo_op());

        let input = OperatorInput::new(Content::text("stream test"), TriggerType::Task);
        let ctx = dispatch_ctx("echo");

        let mut handle = router.handle(input, &ctx).await.expect("handle ok");
        let event = handle.recv().await.expect("must receive event");

        match event {
            DispatchEvent::Completed { output } => {
                assert_eq!(output.message, Content::text("stream test"));
            }
            other => panic!("expected Completed, got {other:?}"),
        }

        assert!(handle.recv().await.is_none(), "no further events expected");
    }
}
