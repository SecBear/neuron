//! LocalOrchestrator — in-process orchestrator with a HashMap of agents.

use crate::effect::SignalPayload;
use crate::error::OrchError;
use crate::id::{AgentId, WorkflowId};
use crate::operator::{Operator, OperatorInput, OperatorOutput};
use crate::orchestrator::{Orchestrator, QueryPayload};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// In-process orchestrator that dispatches operator invocations to registered agents.
/// Uses `Arc<dyn Operator>` for true concurrent dispatch via `tokio::spawn`.
pub struct LocalOrchestrator {
    agents: HashMap<String, Arc<dyn Operator>>,
}

impl LocalOrchestrator {
    /// Create a new empty orchestrator.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Register an agent with the orchestrator.
    pub fn register(&mut self, id: AgentId, operator: Arc<dyn Operator>) {
        self.agents.insert(id.0, operator);
    }
}

impl Default for LocalOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Orchestrator for LocalOrchestrator {
    async fn dispatch(
        &self,
        agent: &AgentId,
        input: OperatorInput,
    ) -> Result<OperatorOutput, OrchError> {
        let operator = self
            .agents
            .get(agent.as_str())
            .ok_or_else(|| OrchError::AgentNotFound(agent.to_string()))?;
        operator
            .execute(input)
            .await
            .map_err(OrchError::OperatorError)
    }

    async fn dispatch_many(
        &self,
        tasks: Vec<(AgentId, OperatorInput)>,
    ) -> Vec<Result<OperatorOutput, OrchError>> {
        let mut handles = Vec::with_capacity(tasks.len());

        for (agent_id, input) in tasks {
            match self.agents.get(agent_id.as_str()) {
                Some(operator) => {
                    let operator = Arc::clone(operator);
                    handles.push(tokio::spawn(async move {
                        operator
                            .execute(input)
                            .await
                            .map_err(OrchError::OperatorError)
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

    async fn signal(&self, _target: &WorkflowId, _signal: SignalPayload) -> Result<(), OrchError> {
        // LocalOrchestrator doesn't track running workflows
        Ok(())
    }

    async fn query(
        &self,
        _target: &WorkflowId,
        _query: QueryPayload,
    ) -> Result<serde_json::Value, OrchError> {
        // LocalOrchestrator doesn't track running workflows
        Ok(serde_json::Value::Null)
    }
}
