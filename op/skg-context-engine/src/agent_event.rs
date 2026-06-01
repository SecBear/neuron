//! Agent system event vocabulary.
//!
//! [`AgentEvent`] enumerates every meaningful state transition in the agent
//! loop. [`EventKind`] is the discriminant used by
//! [`Trigger::Event`](crate::Trigger) for type-safe pattern matching without
//! binding event payloads.

use layer0::ProtocolError;
use layer0::id::OperatorId;
use layer0::operator::{OperatorInput, OperatorOutput, Outcome};
use rust_decimal::Decimal;
use serde_json::Value;

/// Discriminant for [`AgentEvent`] variants.
///
/// Used in [`Trigger::Event`](crate::Trigger) to match events by type
/// without binding their payloads.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// See [`AgentEvent::BeforeInference`].
    BeforeInference,
    /// See [`AgentEvent::AfterInference`].
    AfterInference,
    /// See [`AgentEvent::ActionRequested`].
    ActionRequested,
    /// See [`AgentEvent::ActionCompleted`].
    ActionCompleted,
    /// See [`AgentEvent::ActionFailed`].
    ActionFailed,
    /// See [`AgentEvent::TokenThreshold`].
    TokenThreshold,
    /// See [`AgentEvent::TurnThreshold`].
    TurnThreshold,
    /// See [`AgentEvent::BudgetThreshold`].
    BudgetThreshold,
    /// See [`AgentEvent::Timer`].
    Timer,
    /// See [`AgentEvent::Timeout`].
    Timeout,
    /// See [`AgentEvent::Signal`].
    Signal,
    /// See [`AgentEvent::LoopStarted`].
    LoopStarted,
    /// See [`AgentEvent::LoopEnding`].
    LoopEnding,
    /// See [`AgentEvent::Custom`].
    Custom,
}

/// Classification of an execution timeout.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeoutKind {
    /// Absolute wall-clock deadline elapsed.
    Deadline,
    /// Per-turn time budget exceeded.
    TurnBudget,
    /// Tool execution time limit hit.
    ToolExecution,
    /// Application-defined timeout category.
    Custom(String),
}

/// Every meaningful event that can occur in the agent loop.
///
/// Emitted by the orchestrator as it drives the infer/act/observe cycle.
/// [`ContextOp`](crate::ContextOp) implementations declare which events they
/// care about by returning a matching [`Trigger`](crate::Trigger) from their
/// [`trigger`](crate::ContextOp::trigger) method.
#[non_exhaustive]
#[derive(Debug)]
pub enum AgentEvent {
    // ── Inference lifecycle ───────────────────────────────────────────────────
    /// The loop is about to dispatch an inference call.
    BeforeInference,

    /// Inference completed; the full model response is attached.
    AfterInference {
        /// The model's response.
        response: skg_turn::InferResponse,
    },

    // ── Action lifecycle ──────────────────────────────────────────────────────
    /// The model requested a tool or sub-operator call.
    ActionRequested {
        /// Operator identifier for the requested action.
        id: OperatorId,
        /// Input payload for the operator.
        input: OperatorInput,
    },

    /// A previously requested action completed successfully.
    ActionCompleted {
        /// Operator identifier.
        id: OperatorId,
        /// Output produced by the operator.
        output: OperatorOutput,
    },

    /// A previously requested action failed.
    ActionFailed {
        /// Operator identifier.
        id: OperatorId,
        /// Protocol-level error describing the failure.
        error: ProtocolError,
    },

    // ── Context thresholds ────────────────────────────────────────────────────
    /// Token count crossed a configured monitoring threshold.
    TokenThreshold {
        /// Current token count at the time of the threshold crossing.
        count: usize,
        /// The configured threshold limit.
        limit: usize,
    },

    /// Turn count crossed a configured monitoring threshold.
    TurnThreshold {
        /// Current turn count.
        count: u32,
        /// The configured threshold limit.
        limit: u32,
    },

    /// Cumulative cost crossed a configured budget threshold.
    BudgetThreshold {
        /// Cumulative spend so far (USD).
        spent: Decimal,
        /// The configured budget limit (USD).
        limit: Decimal,
    },

    // ── Time ──────────────────────────────────────────────────────────────────
    /// A named timer fired.
    Timer {
        /// Identifier of the timer that fired.
        id: String,
    },

    /// An execution timeout occurred.
    Timeout {
        /// Classification of the timeout.
        kind: TimeoutKind,
    },

    // ── External ─────────────────────────────────────────────────────────────
    /// An external signal was received by the agent.
    Signal {
        /// Signal kind label.
        kind: String,
        /// Signal payload.
        payload: Value,
    },

    // ── Lifecycle ─────────────────────────────────────────────────────────────
    /// The agent loop has started its first iteration.
    LoopStarted,

    /// The agent loop is about to terminate.
    LoopEnding {
        /// The outcome driving termination.
        outcome: Outcome,
    },

    // ── Escape hatch ──────────────────────────────────────────────────────────
    /// A custom, application-defined event not covered by the standard set.
    Custom {
        /// Application-defined event kind label.
        kind: String,
        /// Event payload.
        payload: Value,
    },
}

impl AgentEvent {
    /// Return the [`EventKind`] discriminant for this event.
    ///
    /// Used by [`Trigger::Event`](crate::Trigger) to match events without
    /// binding payloads.
    pub fn kind(&self) -> EventKind {
        match self {
            Self::BeforeInference => EventKind::BeforeInference,
            Self::AfterInference { .. } => EventKind::AfterInference,
            Self::ActionRequested { .. } => EventKind::ActionRequested,
            Self::ActionCompleted { .. } => EventKind::ActionCompleted,
            Self::ActionFailed { .. } => EventKind::ActionFailed,
            Self::TokenThreshold { .. } => EventKind::TokenThreshold,
            Self::TurnThreshold { .. } => EventKind::TurnThreshold,
            Self::BudgetThreshold { .. } => EventKind::BudgetThreshold,
            Self::Timer { .. } => EventKind::Timer,
            Self::Timeout { .. } => EventKind::Timeout,
            Self::Signal { .. } => EventKind::Signal,
            Self::LoopStarted => EventKind::LoopStarted,
            Self::LoopEnding { .. } => EventKind::LoopEnding,
            Self::Custom { .. } => EventKind::Custom,
        }
    }
}
