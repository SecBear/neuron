#![deny(missing_docs)]
//! Local (passthrough) implementation of layer0's Environment trait.
//!
//! No isolation, no credential injection, no resource limits.
//! Executes the operator directly in the current process. The operator
//! is provided at construction time and stored as an `Arc<dyn Operator>`.

use async_trait::async_trait;
use layer0::environment::{Environment, EnvironmentSpec};
use layer0::error::EnvError;
use layer0::operator::{Operator, OperatorInput, OperatorOutput};
use std::sync::Arc;

/// Local passthrough environment.
///
/// Owns an `Arc<dyn Operator>` and delegates directly to it.
/// The `EnvironmentSpec` is accepted but ignored — there is no
/// isolation, credential injection, or resource limiting.
///
/// Suitable for development, testing, and single-process deployments
/// where isolation is not required.
pub struct LocalEnv {
    op: Arc<dyn Operator>,
}

impl LocalEnv {
    /// Create a new local environment wrapping the given operator.
    pub fn new(op: Arc<dyn Operator>) -> Self {
        Self { op }
    }
}

#[async_trait]
impl Environment for LocalEnv {
    async fn run(
        &self,
        input: OperatorInput,
        _spec: &EnvironmentSpec,
    ) -> Result<OperatorOutput, EnvError> {
        self.op
            .execute(input)
            .await
            .map_err(EnvError::OperatorError)
    }
}
