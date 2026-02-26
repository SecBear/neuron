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

#[cfg(test)]
mod tests {
    use super::*;
    use layer0::content::Content;
    use layer0::error::OperatorError;
    use layer0::operator::{ExitReason, OperatorOutput, TriggerType};

    struct EchoOperator;

    #[async_trait]
    impl Operator for EchoOperator {
        async fn execute(&self, input: OperatorInput) -> Result<OperatorOutput, OperatorError> {
            Ok(OperatorOutput::new(input.message, ExitReason::Complete))
        }
    }

    struct FailOperator;

    #[async_trait]
    impl Operator for FailOperator {
        async fn execute(&self, _input: OperatorInput) -> Result<OperatorOutput, OperatorError> {
            Err(OperatorError::Model("deliberate failure".into()))
        }
    }

    #[tokio::test]
    async fn local_env_delegates_to_operator() {
        let op: Arc<dyn Operator> = Arc::new(EchoOperator);
        let env = LocalEnv::new(op);

        let input = OperatorInput::new(Content::text("hello"), TriggerType::User);
        let spec = EnvironmentSpec::default();

        let output = env.run(input, &spec).await.unwrap();
        assert_eq!(output.exit_reason, ExitReason::Complete);
        assert_eq!(output.message.as_text().unwrap(), "hello");
    }

    #[tokio::test]
    async fn local_env_propagates_operator_error() {
        let op: Arc<dyn Operator> = Arc::new(FailOperator);
        let env = LocalEnv::new(op);

        let input = OperatorInput::new(Content::text("hello"), TriggerType::User);
        let spec = EnvironmentSpec::default();

        let result = env.run(input, &spec).await;
        assert!(result.is_err());
    }

    #[test]
    fn local_env_implements_environment() {
        fn _assert_env<T: Environment>() {}
        _assert_env::<LocalEnv>();
    }
}
