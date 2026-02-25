use layer0::content::Content;
use layer0::environment::{Environment, EnvironmentSpec};
use layer0::error::EnvError;
use layer0::operator::{OperatorInput, OperatorOutput, TriggerType};
use layer0::test_utils::EchoOperator;
use neuron_env_local::LocalEnv;
use std::sync::Arc;

fn simple_input(msg: &str) -> OperatorInput {
    OperatorInput::new(Content::text(msg), TriggerType::User)
}

// --- Basic execution ---

#[tokio::test]
async fn passthrough_execution() {
    let env = LocalEnv::new(Arc::new(EchoOperator));
    let input = simple_input("hello");
    let spec = EnvironmentSpec::default();

    let output = env.run(input, &spec).await.unwrap();
    assert_eq!(output.message, Content::text("hello"));
}

#[tokio::test]
async fn preserves_operator_metadata() {
    let env = LocalEnv::new(Arc::new(EchoOperator));
    let input = simple_input("test");
    let spec = EnvironmentSpec::default();

    let output = env.run(input, &spec).await.unwrap();
    // EchoOperator returns default metadata
    assert_eq!(output.metadata.tokens_in, 0);
}

// --- Error propagation ---

/// An operator that always fails.
struct FailingOperator;

#[async_trait::async_trait]
impl layer0::operator::Operator for FailingOperator {
    async fn execute(
        &self,
        _input: OperatorInput,
    ) -> Result<OperatorOutput, layer0::error::OperatorError> {
        Err(layer0::error::OperatorError::NonRetryable(
            "always fails".into(),
        ))
    }
}

#[tokio::test]
async fn propagates_operator_error() {
    let env = LocalEnv::new(Arc::new(FailingOperator));
    let input = simple_input("will fail");
    let spec = EnvironmentSpec::default();

    let result = env.run(input, &spec).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        EnvError::OperatorError(e) => {
            assert_eq!(e.to_string(), "non-retryable: always fails");
        }
        other => panic!("expected OperatorError, got: {other}"),
    }
}

// --- Object safety ---

#[tokio::test]
async fn usable_as_box_dyn_environment() {
    let env: Box<dyn Environment> = Box::new(LocalEnv::new(Arc::new(EchoOperator)));
    let input = simple_input("dyn test");
    let spec = EnvironmentSpec::default();

    let output = env.run(input, &spec).await.unwrap();
    assert_eq!(output.message, Content::text("dyn test"));
}

#[tokio::test]
async fn usable_as_arc_dyn_environment() {
    let env: Arc<dyn Environment> = Arc::new(LocalEnv::new(Arc::new(EchoOperator)));
    let input = simple_input("arc test");
    let spec = EnvironmentSpec::default();

    let output = env.run(input, &spec).await.unwrap();
    assert_eq!(output.message, Content::text("arc test"));
}

// --- Spec is ignored (passthrough) ---

#[tokio::test]
async fn ignores_spec_fields() {
    let env = LocalEnv::new(Arc::new(EchoOperator));
    let input = simple_input("spec ignored");
    let spec = EnvironmentSpec::default();

    // LocalEnv ignores the spec — it's a passthrough
    let output = env.run(input, &spec).await.unwrap();
    assert_eq!(output.message, Content::text("spec ignored"));
}
