//! EchoOperator — returns the input message as the output.

use crate::error::OperatorError;
use crate::operator::{ExitReason, OperatorInput, OperatorMetadata, OperatorOutput};
use async_trait::async_trait;

/// An operator implementation that echoes the input message back as output.
/// Used for testing orchestration, environment, and hook integrations.
pub struct EchoOperator;

#[async_trait]
impl crate::operator::Operator for EchoOperator {
    async fn execute(&self, input: OperatorInput) -> Result<OperatorOutput, OperatorError> {
        Ok(OperatorOutput {
            message: input.message,
            exit_reason: ExitReason::Complete,
            metadata: OperatorMetadata::default(),
            effects: vec![],
        })
    }
}
