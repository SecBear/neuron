//! LocalEnvironment — no isolation, just passthrough to the turn.

use crate::environment::EnvironmentSpec;
use crate::error::EnvError;
use crate::turn::{Turn, TurnInput, TurnOutput};
use async_trait::async_trait;
use std::sync::Arc;

/// A passthrough environment that executes the turn directly with no isolation.
/// Used for local development and testing. The turn is provided at construction
/// time and stored internally — callers don't pass it on every run() call.
pub struct LocalEnvironment {
    turn: Arc<dyn Turn>,
}

impl LocalEnvironment {
    /// Create a new local environment wrapping the given turn.
    pub fn new(turn: Arc<dyn Turn>) -> Self {
        Self { turn }
    }
}

#[async_trait]
impl crate::environment::Environment for LocalEnvironment {
    async fn run(
        &self,
        input: TurnInput,
        _spec: &EnvironmentSpec,
    ) -> Result<TurnOutput, EnvError> {
        self.turn.execute(input).await.map_err(EnvError::TurnError)
    }
}
