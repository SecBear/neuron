#![deny(missing_docs)]
//! In-process implementation of layer0's Orchestrator trait.
//!
//! Dispatches turns to registered agents via `HashMap<AgentId, Arc<dyn Turn>>`.
//! Concurrent dispatch uses `tokio::spawn`. No durability — turns that fail
//! are not retried and state is not persisted. Signal and query are no-ops.

use async_trait::async_trait;
use layer0::effect::SignalPayload;
use layer0::error::OrchError;
use layer0::id::{AgentId, WorkflowId};
use layer0::orchestrator::{Orchestrator, QueryPayload};
use layer0::turn::{Turn, TurnInput, TurnOutput};
use std::collections::HashMap;
use std::sync::Arc;

/// In-process orchestrator that dispatches turns to registered agents.
///
/// Uses `Arc<dyn Turn>` for true concurrent dispatch via `tokio::spawn`.
/// No durability, no workflow tracking. Suitable for development,
/// testing, and single-process deployments.
pub struct LocalOrch {
    agents: HashMap<String, Arc<dyn Turn>>,
}

impl LocalOrch {
    /// Create a new empty orchestrator.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Register an agent with the orchestrator.
    pub fn register(&mut self, id: AgentId, turn: Arc<dyn Turn>) {
        self.agents.insert(id.to_string(), turn);
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
        input: TurnInput,
    ) -> Result<TurnOutput, OrchError> {
        let turn = self
            .agents
            .get(agent.as_str())
            .ok_or_else(|| OrchError::AgentNotFound(agent.to_string()))?;
        turn.execute(input).await.map_err(OrchError::TurnError)
    }

    async fn dispatch_many(
        &self,
        tasks: Vec<(AgentId, TurnInput)>,
    ) -> Vec<Result<TurnOutput, OrchError>> {
        let mut handles = Vec::with_capacity(tasks.len());

        for (agent_id, input) in tasks {
            match self.agents.get(agent_id.as_str()) {
                Some(turn) => {
                    let turn = Arc::clone(turn);
                    handles.push(tokio::spawn(async move {
                        turn.execute(input).await.map_err(OrchError::TurnError)
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
