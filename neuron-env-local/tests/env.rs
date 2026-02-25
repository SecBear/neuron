use layer0::content::Content;
use layer0::environment::{Environment, EnvironmentSpec};
use layer0::error::EnvError;
use layer0::test_utils::EchoTurn;
use layer0::turn::{TriggerType, TurnInput, TurnOutput};
use neuron_env_local::LocalEnv;
use std::sync::Arc;

fn simple_input(msg: &str) -> TurnInput {
    TurnInput::new(Content::text(msg), TriggerType::User)
}

// --- Basic execution ---

#[tokio::test]
async fn passthrough_execution() {
    let env = LocalEnv::new(Arc::new(EchoTurn));
    let input = simple_input("hello");
    let spec = EnvironmentSpec::default();

    let output = env.run(input, &spec).await.unwrap();
    assert_eq!(output.message, Content::text("hello"));
}

#[tokio::test]
async fn preserves_turn_metadata() {
    let env = LocalEnv::new(Arc::new(EchoTurn));
    let input = simple_input("test");
    let spec = EnvironmentSpec::default();

    let output = env.run(input, &spec).await.unwrap();
    // EchoTurn returns default metadata
    assert_eq!(output.metadata.tokens_in, 0);
}

// --- Error propagation ---

/// A turn that always fails.
struct FailingTurn;

#[async_trait::async_trait]
impl layer0::turn::Turn for FailingTurn {
    async fn execute(
        &self,
        _input: TurnInput,
    ) -> Result<TurnOutput, layer0::error::TurnError> {
        Err(layer0::error::TurnError::NonRetryable(
            "always fails".into(),
        ))
    }
}

#[tokio::test]
async fn propagates_turn_error() {
    let env = LocalEnv::new(Arc::new(FailingTurn));
    let input = simple_input("will fail");
    let spec = EnvironmentSpec::default();

    let result = env.run(input, &spec).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        EnvError::TurnError(e) => {
            assert_eq!(e.to_string(), "non-retryable: always fails");
        }
        other => panic!("expected TurnError, got: {other}"),
    }
}

// --- Object safety ---

#[tokio::test]
async fn usable_as_box_dyn_environment() {
    let env: Box<dyn Environment> = Box::new(LocalEnv::new(Arc::new(EchoTurn)));
    let input = simple_input("dyn test");
    let spec = EnvironmentSpec::default();

    let output = env.run(input, &spec).await.unwrap();
    assert_eq!(output.message, Content::text("dyn test"));
}

#[tokio::test]
async fn usable_as_arc_dyn_environment() {
    let env: Arc<dyn Environment> = Arc::new(LocalEnv::new(Arc::new(EchoTurn)));
    let input = simple_input("arc test");
    let spec = EnvironmentSpec::default();

    let output = env.run(input, &spec).await.unwrap();
    assert_eq!(output.message, Content::text("arc test"));
}

// --- Spec is ignored (passthrough) ---

#[tokio::test]
async fn ignores_spec_fields() {
    let env = LocalEnv::new(Arc::new(EchoTurn));
    let input = simple_input("spec ignored");
    let spec = EnvironmentSpec::default();

    // LocalEnv ignores the spec — it's a passthrough
    let output = env.run(input, &spec).await.unwrap();
    assert_eq!(output.message, Content::text("spec ignored"));
}
