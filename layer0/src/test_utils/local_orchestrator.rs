//! LocalOrchestrator — in-process operator router with a HashMap of child operators.

use crate::capability::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind, ExecutionClass,
    SchedulingFacts,
};
use crate::dispatch::DispatchHandle;
use crate::dispatch_context::DispatchContext;
use crate::error::ProtocolError;
use crate::id::OperatorId;
use crate::operator::{Operator, OperatorInput};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// In-process operator router that dispatches invocations to registered child operators.
///
/// Acts as an `Operator` itself: `descriptor()` advertises it as a router, and
/// `handle()` uses `ctx.operator_id` to look up and delegate to the matching child.
pub struct LocalOrchestrator {
    operators: HashMap<String, Arc<dyn Operator>>,
}

impl LocalOrchestrator {
    /// Create a new empty orchestrator.
    pub fn new() -> Self {
        Self {
            operators: HashMap::new(),
        }
    }

    /// Register a child operator with the orchestrator.
    pub fn register(&mut self, id: OperatorId, operator: Arc<dyn Operator>) {
        self.operators.insert(id.0, operator);
    }
}

impl Default for LocalOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Operator for LocalOrchestrator {
    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new(
            CapabilityId::new("test.local_orchestrator"),
            CapabilityKind::Service,
            "local_orchestrator",
            "Routes invocations to registered child operators by operator_id",
            SchedulingFacts::new(ExecutionClass::Shared, false, false, false, None),
            ApprovalFacts::None,
            AuthFacts::Open,
        )
    }

    async fn handle(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<DispatchHandle, ProtocolError> {
        let op = self
            .operators
            .get(ctx.operator_id.as_str())
            .ok_or_else(|| {
                ProtocolError::not_found(format!("operator not found: {}", ctx.operator_id))
            })?
            .clone();

        op.handle(input, ctx).await
    }
}
