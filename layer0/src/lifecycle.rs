//! Lifecycle events — cross-protocol coordination vocabulary.
//!
//! These are NOT a trait — they're a shared vocabulary. Each protocol
//! emits and/or consumes these events through whatever mechanism
//! is appropriate (channels, callbacks, event bus, direct calls).
//!
//! The Lifecycle Interface is deliberately not a trait because
//! lifecycle coordination is the orchestrator's job. The orchestrator
//! listens for events, applies policies, and takes action. There's
//! no separate "lifecycle service" — it's a responsibility of
//! the orchestration layer.
//!
//! ## Future: Sub-Turn Event Streaming
//!
//! The current lifecycle events are coarse-grained (budget, compaction,
//! observability). A future extension may add a sub-turn event stream
//! for real-time observation of individual inference calls, tool
//! executions, and context mutations within a turn — enabling live
//! dashboards and fine-grained telemetry beyond the Hook interface.

use crate::{content::Content, duration::DurationMs, effect::Scope, id::*};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Budget-related events.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BudgetEvent {
    /// Emitted by turn after each model call.
    CostIncurred {
        /// The agent that incurred the cost.
        agent: AgentId,
        /// Cost of this individual operation.
        cost: Decimal,
        /// Cumulative cost so far.
        cumulative: Decimal,
    },
    /// Emitted by orchestrator when nearing limit.
    BudgetWarning {
        /// The workflow approaching its budget limit.
        workflow: WorkflowId,
        /// Amount spent so far.
        spent: Decimal,
        /// The budget limit.
        limit: Decimal,
    },
    /// Decision by orchestrator.
    BudgetAction {
        /// The workflow the decision applies to.
        workflow: WorkflowId,
        /// The budget decision.
        action: BudgetDecision,
    },
}

/// What the orchestrator decides to do about budget pressure.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetDecision {
    /// Continue as normal.
    Continue,
    /// Switch to a cheaper model.
    DowngradeModel {
        /// The model being switched from.
        from: String,
        /// The model being switched to.
        to: String,
    },
    /// Stop the entire workflow.
    HaltWorkflow,
    /// Request more budget from the caller.
    RequestIncrease {
        /// The additional amount requested.
        amount: Decimal,
    },
}

/// Context pressure events — for compaction coordination.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompactionEvent {
    /// Emitted by turn when context window is filling.
    ContextPressure {
        /// The agent experiencing context pressure.
        agent: AgentId,
        /// Percentage of context window used.
        fill_percent: f64,
        /// Tokens currently used.
        tokens_used: u64,
        /// Tokens still available.
        tokens_available: u64,
    },
    /// Emitted before compaction to trigger memory flush.
    PreCompactionFlush {
        /// The agent about to compact.
        agent: AgentId,
        /// The scope to flush.
        scope: Scope,
    },
    /// Emitted after compaction completes.
    CompactionComplete {
        /// The agent that completed compaction.
        agent: AgentId,
        /// The compaction strategy used.
        strategy: String,
        /// Number of tokens freed.
        tokens_freed: u64,
    },
    /// Compaction was initiated and executed by the model provider
    /// (e.g., Anthropic's server-side context compaction). The turn
    /// receives a compacted context back from the inference call itself.
    ProviderManaged {
        /// The agent whose context was compacted.
        agent: AgentId,
        /// The provider that performed the compaction.
        provider: String,
        /// Token count before compaction.
        tokens_before: u64,
        /// Token count after compaction.
        tokens_after: u64,
        /// Optional summary content produced by the provider.
        summary: Option<Content>,
    },
}

/// Observability events — the common vocabulary all layers emit.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservableEvent {
    /// Which protocol emitted this.
    pub source: EventSource,
    /// Event type (free-form, namespaced by convention).
    pub event_type: String,
    /// When it happened (milliseconds since workflow start, not wall clock).
    pub timestamp: DurationMs,
    /// Event payload.
    pub data: serde_json::Value,
    /// Correlation ID across protocols.
    pub trace_id: Option<String>,
    /// Workflow context.
    pub workflow_id: Option<WorkflowId>,
    /// Agent context.
    pub agent_id: Option<AgentId>,
}

/// Which protocol layer emitted an event.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    /// From the Turn protocol.
    Turn,
    /// From the Orchestration protocol.
    Orchestration,
    /// From the State protocol.
    State,
    /// From the Environment protocol.
    Environment,
    /// From a Hook.
    Hook,
}

impl ObservableEvent {
    /// Create a new observable event with required fields.
    pub fn new(
        source: EventSource,
        event_type: impl Into<String>,
        timestamp: DurationMs,
        data: serde_json::Value,
    ) -> Self {
        Self {
            source,
            event_type: event_type.into(),
            timestamp,
            data,
            trace_id: None,
            workflow_id: None,
            agent_id: None,
        }
    }
}
