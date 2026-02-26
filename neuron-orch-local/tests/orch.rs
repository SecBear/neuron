use layer0::content::Content;
use layer0::id::{AgentId, WorkflowId};
use layer0::operator::{OperatorInput, OperatorOutput, TriggerType};
use layer0::orchestrator::{Orchestrator, QueryPayload};
use layer0::test_utils::EchoOperator;
use neuron_orch_local::LocalOrch;
use std::sync::Arc;

fn simple_input(msg: &str) -> OperatorInput {
    OperatorInput::new(Content::text(msg), TriggerType::User)
}

// --- Single dispatch ---

#[tokio::test]
async fn dispatch_to_registered_agent() {
    let mut orch = LocalOrch::new();
    orch.register(AgentId::new("echo"), Arc::new(EchoOperator));

    let output = orch
        .dispatch(&AgentId::new("echo"), simple_input("hello"))
        .await
        .unwrap();
    assert_eq!(output.message, Content::text("hello"));
}

#[tokio::test]
async fn dispatch_agent_not_found() {
    let orch = LocalOrch::new();

    let result = orch
        .dispatch(&AgentId::new("missing"), simple_input("fail"))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("agent not found"));
}

// --- Error propagation ---

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
async fn dispatch_propagates_operator_error() {
    let mut orch = LocalOrch::new();
    orch.register(AgentId::new("fail"), Arc::new(FailingOperator));

    let result = orch
        .dispatch(&AgentId::new("fail"), simple_input("boom"))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("always fails"));
}

// --- Dispatch many ---

#[tokio::test]
async fn dispatch_many_concurrent() {
    let mut orch = LocalOrch::new();
    orch.register(AgentId::new("a"), Arc::new(EchoOperator));
    orch.register(AgentId::new("b"), Arc::new(EchoOperator));

    let tasks = vec![
        (AgentId::new("a"), simple_input("msg-a")),
        (AgentId::new("b"), simple_input("msg-b")),
    ];

    let results = orch.dispatch_many(tasks).await;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].as_ref().unwrap().message, Content::text("msg-a"));
    assert_eq!(results[1].as_ref().unwrap().message, Content::text("msg-b"));
}

#[tokio::test]
async fn dispatch_many_partial_failure() {
    let mut orch = LocalOrch::new();
    orch.register(AgentId::new("ok"), Arc::new(EchoOperator));
    // "bad" is not registered

    let tasks = vec![
        (AgentId::new("ok"), simple_input("fine")),
        (AgentId::new("bad"), simple_input("fail")),
    ];

    let results = orch.dispatch_many(tasks).await;
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
}

// --- Signal and query ---

#[tokio::test]
async fn signal_accepted() {
    let orch = LocalOrch::new();
    let wf = WorkflowId::new("wf-1");
    let signal =
        layer0::effect::SignalPayload::new("cancel", serde_json::json!({"reason": "user request"}));
    let result = orch.signal(&wf, signal).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn query_returns_null() {
    let orch = LocalOrch::new();
    let wf = WorkflowId::new("wf-1");
    let query = QueryPayload::new("status", serde_json::json!({}));
    let result = orch.query(&wf, query).await.unwrap();
    assert_eq!(result, serde_json::Value::Null);
}

// --- Object safety ---

#[tokio::test]
async fn usable_as_dyn_orchestrator() {
    let mut orch = LocalOrch::new();
    orch.register(AgentId::new("echo"), Arc::new(EchoOperator));

    let orch: Box<dyn Orchestrator> = Box::new(orch);
    let output = orch
        .dispatch(&AgentId::new("echo"), simple_input("dyn"))
        .await
        .unwrap();
    assert_eq!(output.message, Content::text("dyn"));
}

#[tokio::test]
async fn usable_as_arc_dyn_orchestrator() {
    let mut orch = LocalOrch::new();
    orch.register(AgentId::new("echo"), Arc::new(EchoOperator));

    let orch: Arc<dyn Orchestrator> = Arc::new(orch);
    let output = orch
        .dispatch(&AgentId::new("echo"), simple_input("arc"))
        .await
        .unwrap();
    assert_eq!(output.message, Content::text("arc"));
}
