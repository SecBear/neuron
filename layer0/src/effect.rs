//! Effect system — side-effects declared by turns for external execution.

use crate::id::*;
use serde::{Deserialize, Serialize};

/// A side-effect declared by a turn. NOT executed by the turn —
/// the calling layer decides when and how to execute it.
///
/// This is the key composability mechanism. A turn running in-process
/// has its effects executed by a simple loop. A turn running in Temporal
/// has its effects serialized into the workflow history. A turn running
/// in a test harness has its effects captured for assertions.
///
/// The Custom variant ensures future effect types can be represented
/// without changing the enum. When a new effect type stabilizes
/// (used by 3+ implementations), it graduates to a named variant.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Write a value to persistent state.
    WriteMemory {
        /// The scope to write into.
        scope: Scope,
        /// The key to write.
        key: String,
        /// The value to store.
        value: serde_json::Value,
    },

    /// Delete a value from persistent state.
    DeleteMemory {
        /// The scope to delete from.
        scope: Scope,
        /// The key to delete.
        key: String,
    },

    /// Send a fire-and-forget signal to another agent or workflow.
    Signal {
        /// The target workflow to signal.
        target: WorkflowId,
        /// The signal payload.
        payload: SignalPayload,
    },

    /// Request that the orchestrator dispatch another agent.
    /// This is how delegation works — the turn doesn't call the
    /// other agent directly, it asks the orchestrator to do it.
    Delegate {
        /// The agent to delegate to.
        agent: AgentId,
        /// The input to send to the delegated agent.
        input: Box<TurnInput>,
    },

    /// Hand off the conversation to another agent. Unlike Delegate,
    /// the current turn is done — the next agent takes over.
    Handoff {
        /// The agent to hand off to.
        agent: AgentId,
        /// State to pass to the next agent. This is NOT the full
        /// conversation — it's whatever the current agent thinks
        /// the next agent needs to continue.
        state: serde_json::Value,
    },

    /// Emit a log/trace event. Observers and telemetry consume these.
    Log {
        /// Severity level.
        level: LogLevel,
        /// Log message.
        message: String,
        /// Optional structured data.
        data: Option<serde_json::Value>,
    },

    /// Future effect types. Named string + arbitrary payload.
    /// Use this for domain-specific effects that aren't general
    /// enough for a named variant.
    Custom {
        /// The custom effect type identifier.
        effect_type: String,
        /// Arbitrary payload.
        data: serde_json::Value,
    },
}

// Forward-declare TurnInput usage for the Delegate variant.
use crate::turn::TurnInput;

/// Where state lives. Scopes are hierarchical — a session scope
/// is narrower than a workflow scope, which is narrower than global.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Per-conversation.
    Session(SessionId),
    /// Per-workflow-execution.
    Workflow(WorkflowId),
    /// Per-agent within a workflow.
    Agent {
        /// The workflow this agent belongs to.
        workflow: WorkflowId,
        /// The agent within the workflow.
        agent: AgentId,
    },
    /// Shared across all workflows.
    Global,
    /// Future scopes.
    Custom(String),
}

/// Payload for inter-agent/workflow signals.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalPayload {
    /// The type of signal being sent.
    pub signal_type: String,
    /// Signal data.
    pub data: serde_json::Value,
}

impl SignalPayload {
    /// Create a new signal payload.
    pub fn new(signal_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            signal_type: signal_type.into(),
            data,
        }
    }
}

/// Log severity levels.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Finest-grained tracing.
    Trace,
    /// Debug-level detail.
    Debug,
    /// Informational messages.
    Info,
    /// Warnings.
    Warn,
    /// Errors.
    Error,
}
