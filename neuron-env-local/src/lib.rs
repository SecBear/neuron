#![deny(missing_docs)]
//! Local (passthrough) implementation of layer0's Environment trait.
//!
//! No isolation, no credential injection, no resource limits.
//! Executes the turn directly in the current process. The turn
//! is provided at construction time and stored as an `Arc<dyn Turn>`.

use async_trait::async_trait;
use layer0::environment::{Environment, EnvironmentSpec};
use layer0::error::EnvError;
use layer0::turn::{Turn, TurnInput, TurnOutput};
use std::sync::Arc;

/// Local passthrough environment.
///
/// Owns an `Arc<dyn Turn>` and delegates directly to it.
/// The `EnvironmentSpec` is accepted but ignored — there is no
/// isolation, credential injection, or resource limiting.
///
/// Suitable for development, testing, and single-process deployments
/// where isolation is not required.
pub struct LocalEnv {
    turn: Arc<dyn Turn>,
}

impl LocalEnv {
    /// Create a new local environment wrapping the given turn.
    pub fn new(turn: Arc<dyn Turn>) -> Self {
        Self { turn }
    }
}

#[async_trait]
impl Environment for LocalEnv {
    async fn run(
        &self,
        input: TurnInput,
        _spec: &EnvironmentSpec,
    ) -> Result<TurnOutput, EnvError> {
        self.turn.execute(input).await.map_err(EnvError::TurnError)
    }
}
