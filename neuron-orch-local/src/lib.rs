#![deny(missing_docs)]
//! In-process implementation of layer0's Orchestrator trait.
//!
//! Dispatches to registered agents via `HashMap<AgentId, Arc<dyn Operator>>`.
//! Concurrent dispatch uses `tokio::spawn`. No durability — operators that fail
//! are not retried and state is not persisted. Workflow `signal` and `query`
//! semantics are implemented as an in-memory signal journal.

use async_trait::async_trait;
use layer0::effect::SignalPayload;
use layer0::error::OrchError;
use layer0::id::{AgentId, WorkflowId};
use layer0::operator::{Operator, OperatorInput, OperatorOutput};
use layer0::orchestrator::{Orchestrator, QueryPayload};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-process orchestrator that dispatches to registered agents.
///
/// Uses `Arc<dyn Operator>` for true concurrent dispatch via `tokio::spawn`.
/// No durability, but tracks workflow signals in-memory for `signal`/`query`.
/// Suitable for development, testing, and single-process deployments.
pub struct LocalOrch {
    agents: HashMap<String, Arc<dyn Operator>>,
    workflow_signals: RwLock<HashMap<String, Vec<SignalPayload>>>,
}

impl LocalOrch {
    /// Create a new empty orchestrator.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            workflow_signals: RwLock::new(HashMap::new()),
        }
    }

    /// Register an agent with the orchestrator.
    pub fn register(&mut self, id: AgentId, op: Arc<dyn Operator>) {
        self.agents.insert(id.to_string(), op);
    }
}

impl Default for LocalOrch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Orchestrator for LocalOrch {
    async fn dispatch(
        &self,
        agent: &AgentId,
        input: OperatorInput,
    ) -> Result<OperatorOutput, OrchError> {
        let op = self
            .agents
            .get(agent.as_str())
            .ok_or_else(|| OrchError::AgentNotFound(agent.to_string()))?;
        op.execute(input).await.map_err(OrchError::OperatorError)
    }

    async fn dispatch_many(
        &self,
        tasks: Vec<(AgentId, OperatorInput)>,
    ) -> Vec<Result<OperatorOutput, OrchError>> {
        let mut handles = Vec::with_capacity(tasks.len());

        for (agent_id, input) in tasks {
            match self.agents.get(agent_id.as_str()) {
                Some(op) => {
                    let op = Arc::clone(op);
                    handles.push(tokio::spawn(async move {
                        op.execute(input).await.map_err(OrchError::OperatorError)
                    }));
                }
                None => {
                    let name = agent_id.to_string();
                    handles.push(tokio::spawn(
                        async move { Err(OrchError::AgentNotFound(name)) },
                    ));
                }
            }
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(Err(OrchError::DispatchFailed(e.to_string()))),
            }
        }

        results
    }

    async fn signal(&self, target: &WorkflowId, signal: SignalPayload) -> Result<(), OrchError> {
        let mut workflows = self.workflow_signals.write().await;
        workflows
            .entry(target.to_string())
            .or_default()
            .push(signal);
        Ok(())
    }

    async fn query(
        &self,
        target: &WorkflowId,
        query: QueryPayload,
    ) -> Result<serde_json::Value, OrchError> {
        let workflow_id = target.to_string();
        let workflows = self.workflow_signals.read().await;
        let signals = workflows
            .get(&workflow_id)
            .ok_or_else(|| OrchError::WorkflowNotFound(workflow_id.clone()))?;

        match query.query_type.as_str() {
            "status" => Ok(json!({
                "workflow_id": workflow_id,
                "signal_count": signals.len(),
                "last_signal_type": signals.last().map(|s| s.signal_type.clone()),
            })),
            "signals" => {
                let limit = query
                    .params
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(signals.len(), |v| v as usize);
                let start = signals.len().saturating_sub(limit);
                let entries: Vec<serde_json::Value> = signals[start..]
                    .iter()
                    .map(|s| {
                        json!({
                            "signal_type": s.signal_type.clone(),
                            "data": s.data.clone(),
                        })
                    })
                    .collect();

                Ok(json!({
                    "workflow_id": workflow_id,
                    "signal_count": signals.len(),
                    "signals": entries,
                }))
            }
            other => Err(OrchError::DispatchFailed(format!(
                "unsupported query_type: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
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

    fn simple_input(text: &str) -> OperatorInput {
        OperatorInput::new(Content::text(text), TriggerType::User)
    }

    #[tokio::test]
    async fn dispatch_to_registered_agent() {
        let mut orch = LocalOrch::new();
        orch.register(AgentId::new("echo"), Arc::new(EchoOperator));

        let result = orch
            .dispatch(&AgentId::new("echo"), simple_input("hello"))
            .await;
        let output = result.unwrap();
        assert_eq!(output.exit_reason, ExitReason::Complete);
        assert_eq!(output.message.as_text().unwrap(), "hello");
    }

    #[tokio::test]
    async fn dispatch_to_unregistered_agent_returns_error() {
        let orch = LocalOrch::new();

        let result = orch
            .dispatch(&AgentId::new("missing"), simple_input("hello"))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            OrchError::AgentNotFound(name) => assert_eq!(name, "missing"),
            other => panic!("expected AgentNotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_propagates_operator_error() {
        let mut orch = LocalOrch::new();
        orch.register(AgentId::new("fail"), Arc::new(FailOperator));

        let result = orch
            .dispatch(&AgentId::new("fail"), simple_input("hello"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn dispatch_many_parallel() {
        let mut orch = LocalOrch::new();
        orch.register(AgentId::new("echo"), Arc::new(EchoOperator));

        let tasks = vec![
            (AgentId::new("echo"), simple_input("msg1")),
            (AgentId::new("echo"), simple_input("msg2")),
        ];

        let results = orch.dispatch_many(tasks).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
    }

    #[tokio::test]
    async fn dispatch_many_with_missing_agent() {
        let mut orch = LocalOrch::new();
        orch.register(AgentId::new("echo"), Arc::new(EchoOperator));

        let tasks = vec![
            (AgentId::new("echo"), simple_input("msg1")),
            (AgentId::new("missing"), simple_input("msg2")),
        ];

        let results = orch.dispatch_many(tasks).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
    }

    #[tokio::test]
    async fn signal_is_recorded() {
        let orch = LocalOrch::new();
        orch
            .signal(
                &WorkflowId::new("wf1"),
                layer0::effect::SignalPayload::new("test", serde_json::Value::Null),
            )
            .await
            .unwrap();

        let status = orch
            .query(
                &WorkflowId::new("wf1"),
                QueryPayload::new("status", serde_json::json!({})),
            )
            .await
            .unwrap();
        assert_eq!(status["signal_count"], serde_json::json!(1));
    }

    #[tokio::test]
    async fn query_unknown_workflow_errors() {
        let orch = LocalOrch::new();
        let result = orch
            .query(
                &WorkflowId::new("wf1"),
                QueryPayload::new("test", serde_json::Value::Null),
            )
            .await
            .unwrap_err();
        assert!(result.to_string().contains("workflow not found"));
    }

    #[test]
    fn default_orch_is_empty() {
        let orch = LocalOrch::default();
        let _ = orch;
    }

    #[test]
    fn local_orch_implements_orchestrator() {
        fn _assert_orch<T: Orchestrator>() {}
        _assert_orch::<LocalOrch>();
    }
}
