#![deny(missing_docs)]
//! In-process implementation of layer0's Orchestrator trait.
//!
//! Dispatches to registered agents via `HashMap<AgentId, Arc<dyn Operator>>`.
//! Concurrent dispatch uses `tokio::spawn`. No durability — operators that fail
//! are not retried and state is not persisted. Signal and query are no-ops.

use async_trait::async_trait;
use layer0::effect::SignalPayload;
use layer0::error::OrchError;
use layer0::id::{AgentId, WorkflowId};
use layer0::operator::{Operator, OperatorInput, OperatorOutput};
use layer0::orchestrator::{Orchestrator, QueryPayload};
use std::collections::HashMap;
use std::sync::Arc;

/// In-process orchestrator that dispatches to registered agents.
///
/// Uses `Arc<dyn Operator>` for true concurrent dispatch via `tokio::spawn`.
/// No durability, no workflow tracking. Suitable for development,
/// testing, and single-process deployments.
pub struct LocalOrch {
    agents: HashMap<String, Arc<dyn Operator>>,
}

impl LocalOrch {
    /// Create a new empty orchestrator.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
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
                        op.execute(input)
                            .await
                            .map_err(OrchError::OperatorError)
                    }));
                }
                None => {
                    let name = agent_id.to_string();
                    handles.push(tokio::spawn(async move {
                        Err(OrchError::AgentNotFound(name))
                    }));
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

    async fn signal(
        &self,
        _target: &WorkflowId,
        _signal: SignalPayload,
    ) -> Result<(), OrchError> {
        // LocalOrch doesn't track running workflows — accept and discard.
        Ok(())
    }

    async fn query(
        &self,
        _target: &WorkflowId,
        _query: QueryPayload,
    ) -> Result<serde_json::Value, OrchError> {
        // LocalOrch doesn't track running workflows — return null.
        Ok(serde_json::Value::Null)
    }
}
