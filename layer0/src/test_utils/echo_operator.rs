//! EchoOperator — returns the input message as the output.

use crate::capability::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind, ExecutionClass,
    SchedulingFacts,
};
use crate::dispatch::DispatchHandle;
use crate::dispatch_context::DispatchContext;
use crate::error::ProtocolError;
use crate::operator::{OperatorInput, OperatorOutput, Outcome, TerminalOutcome, completed_handle};
use async_trait::async_trait;

/// An operator implementation that echoes the input message back as output.
/// Used for testing orchestration, environment, and hook integrations.
pub struct EchoOperator;

#[async_trait]
impl crate::operator::Operator for EchoOperator {
    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new(
            CapabilityId::new("test.echo"),
            CapabilityKind::Tool,
            "echo",
            "Echoes input back",
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
        let output = OperatorOutput::new(
            input.message,
            Outcome::Terminal {
                terminal: TerminalOutcome::Completed,
            },
        );
        Ok(completed_handle(ctx.dispatch_id.clone(), output))
    }
}
