//! EchoTurn — returns the input message as the output.

use crate::error::TurnError;
use crate::turn::{ExitReason, TurnInput, TurnMetadata, TurnOutput};
use async_trait::async_trait;

/// A turn implementation that echoes the input message back as output.
/// Used for testing orchestration, environment, and hook integrations.
pub struct EchoTurn;

#[async_trait]
impl crate::turn::Turn for EchoTurn {
    async fn execute(&self, input: TurnInput) -> Result<TurnOutput, TurnError> {
        Ok(TurnOutput {
            message: input.message,
            exit_reason: ExitReason::Complete,
            metadata: TurnMetadata::default(),
            effects: vec![],
        })
    }
}
